//! Read only the public product introduction, without a user's login session.
use std::{collections::HashSet, time::Duration};

use ammonia::{Builder, UrlRelative};
use scraper::{Html, Selector};

const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ProductPageError {
    #[error("invalid DLsite work ID")]
    InvalidId,
    #[error("product page request failed")]
    Request(#[from] reqwest::Error),
    #[error("product page exceeds 4 MiB")]
    TooLarge,
    #[error("product introduction was not found; open the product page on DLsite")]
    Unavailable,
}

pub async fn fetch_content(work_id: &str, locale: &str) -> Result<String, ProductPageError> {
    if !valid_work_id(work_id) {
        return Err(ProductPageError::InvalidId);
    }
    let locale = match locale {
        "zh-TW" => "zh_TW",
        "zh-CN" => "zh_CN",
        "en" => "en_US",
        _ => "ja_JP",
    };
    // The home endpoint redirects to the appropriate store, including all-ages works.
    // Never attach the application's account cookies to these public requests.
    let client = reqwest::Client::builder()
        .https_only(true)
        .cookie_store(true)
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 8
                && attempt.url().host_str() == Some("www.dlsite.com")
                && attempt.url().scheme() == "https"
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()?;
    let url = format!("https://www.dlsite.com/home/work/=/product_id/{work_id}.html");
    let mut response = client
        .get(&url)
        .query(&[("locale", locale)])
        .header(
            reqwest::header::COOKIE,
            format!("adultchecked=1; locale={locale}"),
        )
        .send()
        .await?
        .error_for_status()?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_PAGE_BYTES as u64)
    {
        return Err(ProductPageError::TooLarge);
    }
    let base = response.url().clone();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > MAX_PAGE_BYTES {
            return Err(ProductPageError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    extract_content(&String::from_utf8_lossy(&bytes), &base)
}

fn valid_work_id(id: &str) -> bool {
    let Some(number) = ["RJ", "BJ", "VJ"]
        .iter()
        .find_map(|prefix| id.strip_prefix(prefix))
    else {
        return false;
    };
    (6..=10).contains(&number.len()) && number.bytes().all(|b| b.is_ascii_digit())
}

fn extract_content(page: &str, base: &reqwest::Url) -> Result<String, ProductPageError> {
    let document = Html::parse_document(page);
    // Current site plus the classic product layout. Never fall back to the whole page.
    let section = [
        ".c-productInfo__inner",
        ".work_parts_container",
        "#work_description",
    ]
    .into_iter()
    .find_map(|selector| {
        document
            .select(&Selector::parse(selector).expect("static selector"))
            .next()
    })
    .ok_or(ProductPageError::Unavailable)?;
    let content = Builder::default()
        .add_tags(["section"])
        .url_schemes(HashSet::from(["https"]))
        .url_relative(UrlRelative::RewriteWithBase(base.clone()))
        .link_rel(Some("noopener noreferrer"))
        .clean(&section.inner_html())
        .to_string();
    let fragment = Html::parse_fragment(&content);
    let has_text = fragment
        .root_element()
        .text()
        .any(|text| !text.trim().is_empty());
    let has_image = fragment
        .select(&Selector::parse("img[src]").expect("static selector"))
        .next()
        .is_some();
    if !has_text && !has_image {
        return Err(ProductPageError::Unavailable);
    }
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_introduction_and_removes_active_content_and_site_chrome() {
        let base =
            reqwest::Url::parse("https://www.dlsite.com/maniax/work/=/product_id/RJ123456.html")
                .unwrap();
        let html = extract_content(
            r#"<nav>Account</nav><div class="c-productInfo__inner">
            <section><h3>Track list</h3><p>First<br>Second</p>
            <img src="//img.dlsite.jp/cover.jpg" onerror="alert(1)" style="position:fixed">
            <a href="/home/">Circle</a><a href="javascript:alert(1)">unsafe</a>
            <script>alert(1)</script><iframe src="https://example.com"></iframe>
            <form><input name="password"></form><svg onload="alert(1)"></svg></section></div>
            <footer>Recommended products</footer>"#,
            &base,
        )
        .unwrap();
        assert!(html.contains("Track list"));
        assert!(html.contains("First<br>Second"));
        assert!(html.contains("https://img.dlsite.jp/cover.jpg"));
        assert!(html.contains("href=\"https://www.dlsite.com/home/\""));
        for forbidden in [
            "Account",
            "Recommended",
            "script",
            "iframe",
            "onerror",
            "style=",
            "javascript:",
            "<input",
            "<svg",
        ] {
            assert!(!html.contains(forbidden), "{forbidden}: {html}");
        }
    }

    #[test]
    fn supports_classic_layout_and_reports_unavailable_pages() {
        let base = reqwest::Url::parse("https://www.dlsite.com/").unwrap();
        assert!(extract_content(
            "<div class='work_parts_container'><h3>Introduction</h3><p>Text</p></div>",
            &base
        )
        .unwrap()
        .contains("Introduction"));
        assert!(extract_content("<h1>Age verification</h1>", &base).is_err());
        assert!(extract_content(
            "<div class='c-productInfo__inner'><script>only script</script></div>",
            &base
        )
        .is_err());
        assert!(extract_content(
            "<div class='c-productInfo__inner'><img src='https://img.dlsite.jp/image.jpg'></div>",
            &base
        )
        .is_ok());
        assert!(valid_work_id("RJ01760323"));
        assert!(!valid_work_id("RJ123456/../../login"));
    }

    #[tokio::test]
    #[ignore = "public network verification; no account or purchased media"]
    async fn public_product_introduction() {
        let html = fetch_content("RJ01647600", "ja").await.unwrap();
        assert!(html.contains("<img"));
        assert!(!html.contains("<script"));
        println!(
            "Public product introduction: {} sanitized bytes",
            html.len()
        );
    }
}
