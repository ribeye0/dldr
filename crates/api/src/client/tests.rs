use super::auth::{is_two_factor_location, parse_form_fields, parse_two_factor_challenge};
use super::catalog::detect_works_batch_limit;
use super::catalog::PUBLIC_PRODUCT_URL;
use super::downloads::DOWNLOAD_URL;
use super::downloads::{
    check_download_page_authorization, download_resolution_from_raw_response,
    is_absent_optional_serial_page_status, parse_serial_download_link, parse_serial_numbers,
    parse_split_download_parts, serial_page_url,
};
use super::response::parse_json_body;
use crate::raw::RawResponse;
use crate::*;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::StatusCode;
use serde_json::Value;
use std::collections::BTreeMap;

use super::*;

#[tokio::test]
async fn download_stream_stops_at_login_redirects_and_accepts_binary_partial_content() {
    use std::io::{Read, Write};
    for (reply, authorized) in [
            ("HTTP/1.1 302 Found\r\nLocation: https://login.dlsite.com/login?return=private\r\n", false),
            ("HTTP/1.1 302 Found\r\nLocation: https://www.dlsite.com/home/login/=/skip_register/1\r\n", false),
            ("HTTP/1.1 401 Unauthorized\r\n", false),
            ("HTTP/1.1 206 Partial Content\r\nContent-Type: application/zip\r\nContent-Range: bytes 0-0/42\r\n", true),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = Url::parse(&format!("http://{}/file.zip", listener.local_addr().unwrap())).unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                let mut request = [0; 4096];
                let count = socket.read(&mut request).unwrap();
                assert!(String::from_utf8_lossy(&request[..count]).to_ascii_lowercase().contains("range: bytes=0-0"));
                write!(socket, "{reply}Content-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            });
            let client = DlsiteClient::new(DlsiteClientConfig::default()).unwrap();
            let result = client.open_download_stream(&DownloadStreamRequest { url }, Some(DownloadByteRange::first_byte())).await;
            server.join().unwrap();
            if authorized {
                assert_eq!(result.unwrap().status(), StatusCode::PARTIAL_CONTENT);
            } else {
                assert!(matches!(result, Err(ApiError::NotAuthorized)));
            }
        }
}

#[test]
fn detects_batch_limit_from_headers() {
    let mut headers = HeaderMap::new();
    headers.insert("x-batch-limit", HeaderValue::from_static("50"));

    assert_eq!(detect_works_batch_limit(&headers, ""), Some(50));
}

#[test]
fn detects_batch_limit_from_json_body() {
    let headers = HeaderMap::new();
    let body = r#"{ "error": { "maxBatchSize": 50 } }"#;

    assert_eq!(detect_works_batch_limit(&headers, body), Some(50));
}

#[test]
fn detects_batch_limit_from_page_limit_json_body() {
    let headers = HeaderMap::new();
    let body = r#"{ "user": 1223, "page_limit": 50, "concurrency": 500 }"#;

    assert_eq!(detect_works_batch_limit(&headers, body), Some(50));
}

#[test]
fn classifies_raw_download_responses() {
    let raw = RawResponse {
        url: Url::parse(DOWNLOAD_URL).unwrap(),
        status: 302,
        headers: BTreeMap::new(),
        location: Some(
            Url::parse("https://www.dlsite.com/home/download/=/product_id/RJ123456.html").unwrap(),
        ),
        content_type: None,
        body_snippet: None,
    };

    assert!(matches!(
        download_resolution_from_raw_response(&raw),
        DownloadResolution::Direct { .. }
    ));

    let login = RawResponse {
        location: Some(Url::parse("https://login.dlsite.com/login").unwrap()),
        ..raw.clone()
    };
    assert!(matches!(
        download_resolution_from_raw_response(&login),
        DownloadResolution::Unavailable {
            reason: DownloadUnavailableReason::NotAuthorized
        }
    ));
    assert!(matches!(
        check_download_page_authorization(&login),
        Err(ApiError::NotAuthorized)
    ));

    let raw = RawResponse {
        status: 401,
        body_snippet: Some("unauthorized".to_owned()),
        ..raw
    };

    assert!(matches!(
        download_resolution_from_raw_response(&raw),
        DownloadResolution::Unavailable {
            reason: DownloadUnavailableReason::NotAuthorized
        }
    ));
}

#[test]
fn parses_split_download_links() {
    let page_url =
        Url::parse("https://www.dlsite.com/home/download/split/=/product_id/RJ123456.html")
            .unwrap();
    let body = r#"
            <a href="https://www.dlsite.com/home/download/=/number/2/product_id/RJ123456.html">2</a>
            <a href="https://www.dlsite.com/home/download/=/number/1/product_id/RJ123456.html">1</a>
            <a href="/home/download/split/=/product_id/RJ123456.html">self</a>
        "#;

    let parts = parse_split_download_parts(&page_url, body);

    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].number, 1);
    assert_eq!(parts[1].number, 2);
}

#[test]
fn parses_serial_download_link() {
    let page_url =
        Url::parse("https://www.dlsite.com/home/serial/=/product_id/VJ123456.html").unwrap();
    let body = r#"
            <p class="work_download">
                <a href="https://www.dlsite.com/home/download/=/product_id/VJ123456.html">download</a>
            </p>
        "#;

    let request = parse_serial_download_link(&page_url, body).unwrap();

    assert_eq!(
        request.url.as_str(),
        "https://www.dlsite.com/home/download/=/product_id/VJ123456.html"
    );
}

#[test]
fn parses_serial_numbers() {
    let body = r#"
            <h2>シリアル番号</h2>
            <table>
                <tr>
                    <th>シリアル番号</th>
                    <td><strong class="color_02">ABCD-1234-EFGH</strong></td>
                </tr>
            </table>
        "#;

    let serial_numbers = parse_serial_numbers(body);

    assert_eq!(
        serial_numbers,
        vec![SerialNumber {
            label: "シリアル番号".to_owned(),
            value: "ABCD-1234-EFGH".to_owned()
        }]
    );
}

#[test]
fn builds_serial_page_url() {
    let url = serial_page_url(&WorkId::from("RJ123456")).unwrap();

    assert_eq!(
        url.as_str(),
        "https://www.dlsite.com/home/serial/=/product_id/RJ123456.html"
    );
}

#[test]
fn detects_absent_optional_serial_page_statuses() {
    assert!(is_absent_optional_serial_page_status(302));
    assert!(is_absent_optional_serial_page_status(404));
    assert!(is_absent_optional_serial_page_status(500));
    assert!(!is_absent_optional_serial_page_status(200));
}

#[test]
fn parses_public_product_response_shape() {
    let endpoint = Url::parse(PUBLIC_PRODUCT_URL).unwrap();
    let body = r#"[
            {
                "workno": "RJ01553954",
                "work_name": "Public Work",
                "maker_id": "RG01042231",
                "maker_name": "MELLOW VOICE",
                "work_type": "SOU",
                "work_type_string": "ボイス・ASMR",
                "age_category": 1,
                "age_category_string": "general",
                "image_main": {
                    "url": "//img.dlsite.jp/modpub/images2/work/RJ01553954_img_main.jpg"
                },
                "image_thumb": "//img.dlsite.jp/resize/images2/work/RJ01553954_img_main_240x240.jpg",
                "regist_date": "2026-04-24 00:00:00",
                "update_date": "2026-04-25 09:09:14",
                "contents_file_size": 1705634548,
                "genres": [
                    { "name": "ASMR", "id": 497 }
                ]
            }
        ]"#;

    let products = parse_json_body::<Vec<PublicWork>>(endpoint, body).unwrap();

    assert_eq!(products.len(), 1);
    assert_eq!(products[0].id.as_ref(), "RJ01553954");
    assert_eq!(products[0].name.as_deref(), Some("Public Work"));
    assert_eq!(products[0].maker_name.as_deref(), Some("MELLOW VOICE"));
    assert_eq!(products[0].work_kind.as_deref(), Some("SOU"));
    assert_eq!(
        products[0].image_main_url(),
        Some("//img.dlsite.jp/modpub/images2/work/RJ01553954_img_main.jpg")
    );
    assert_eq!(
        products[0].content_size.as_ref().and_then(Value::as_u64),
        Some(1_705_634_548)
    );
}

/// Trimmed copy of the live challenge page captured with
/// `cargo run -p dldr-api --example probe_login`.
const TWO_FACTOR_PAGE: &str = r#"
        <h1>2段階認証</h1>
        <p>認証アプリ（Google Authenticatorなど）を起動して、認証コードを入力してください。</p>
        <form method="POST" action="https://login.dlsite.com/login/two-factor?user=self">
            <input type="hidden" name="_token" value="2FaCsrfTokenFixtureValue0123456789abcdef" autocomplete="off">
            <div class="loginInputBox">
                <input type="text" autocapitalize="off" style="IME-MODE: disabled" required="required" class="formItem-text" id="form_id" name="otp" value="">
            </div>
            <div class="loginCheckBox">
                <label for="form_save">
                    <input type="checkbox" value="true" id="form_save" name="save_id" class="formItem-check">
                </label>
            </div>
            <div class="formSubmitBox type-one">
                <div class="formSubmit-btn">
                    <button type="submit" class="btn type-clrDefault type-sizeMd"><i>認証してログイン</i></button>
                    <input type="hidden" name="recaptcha_token" id="recaptcha_token">
                </div>
            </div>
        </form>
    "#;

#[test]
fn detects_two_factor_redirect_location() {
    assert!(is_two_factor_location(
        &Url::parse("https://login.dlsite.com/login/two-factor").unwrap()
    ));
    assert!(is_two_factor_location(
        &Url::parse("https://login.dlsite.com/login/two-factor?user=self").unwrap()
    ));
    assert!(!is_two_factor_location(
        &Url::parse("https://login.dlsite.com/login").unwrap()
    ));
    assert!(!is_two_factor_location(
        &Url::parse("https://www.dlsite.com/login/two-factor").unwrap()
    ));
}

#[test]
fn parses_two_factor_challenge_form() {
    let page_url = Url::parse("https://login.dlsite.com/login/two-factor").unwrap();
    let challenge = parse_two_factor_challenge(&page_url, TWO_FACTOR_PAGE).unwrap();

    assert_eq!(
        challenge.action.as_str(),
        "https://login.dlsite.com/login/two-factor?user=self"
    );
    assert_eq!(challenge.code_field, "otp");
    assert_eq!(
        challenge.fields,
        vec![
            (
                "_token".to_owned(),
                "2FaCsrfTokenFixtureValue0123456789abcdef".to_owned()
            ),
            ("recaptcha_token".to_owned(), String::new()),
        ]
    );
}

#[test]
fn two_factor_challenge_debug_hides_field_values() {
    let page_url = Url::parse("https://login.dlsite.com/login/two-factor").unwrap();
    let challenge = parse_two_factor_challenge(&page_url, TWO_FACTOR_PAGE).unwrap();
    let debug = format!("{challenge:?}");

    assert!(debug.contains("_token"));
    assert!(!debug.contains("2FaCsrfTokenFixtureValue0123456789abcdef"));
}

#[test]
fn ignores_pages_without_a_two_factor_form() {
    let page_url = Url::parse("https://login.dlsite.com/login").unwrap();
    let body = r#"
            <form method="POST" action="https://login.dlsite.com/login">
                <input type="hidden" name="_token" value="token">
                <input type="text" name="login_id" value="">
                <input type="password" name="password" value="">
            </form>
        "#;

    assert!(parse_two_factor_challenge(&page_url, body).is_none());
}

#[test]
fn keeps_checked_toggles_and_drops_unchecked_ones() {
    let inner = r#"
            <input type="checkbox" name="unchecked" value="true">
            <input type="checkbox" name="checked" value="true" checked>
            <input type="submit" name="action" value="go">
            <input type="hidden" name="token" value="a&amp;b">
        "#;

    assert_eq!(
        parse_form_fields(inner, "otp"),
        vec![
            ("checked".to_owned(), "true".to_owned()),
            ("token".to_owned(), "a&b".to_owned()),
        ]
    );
}

#[test]
fn exports_and_imports_empty_session_snapshot() {
    let client = DlsiteClient::new(DlsiteClientConfig::default()).unwrap();
    let snapshot = client.export_session().unwrap();

    let other = DlsiteClient::new(DlsiteClientConfig::default()).unwrap();
    other.import_session(&snapshot).unwrap();

    assert_eq!(
        other.export_session().unwrap().cookies_json,
        snapshot.cookies_json
    );
}
