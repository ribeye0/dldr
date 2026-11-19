//! Authentication, cookie snapshots and two-factor form handling.
use super::catalog::CONTENT_COUNT_URL;
use super::response::{
    decode_basic_html_entities, parse_json_body, redirect_location, response_text_snippet,
    text_snippet,
};
use super::{DiagnosticRequest, DlsiteClient};
use crate::{
    raw::RawResponse, ApiError, ContentCount, Credentials, LoginOutcome, Result, SessionSnapshot,
    SessionStatus, TwoFactorChallenge,
};
use reqwest::StatusCode;
use std::io::BufWriter;
use url::Url;

const LOGIN_URL: &str = "https://login.dlsite.com/login";
pub(super) const LOGIN_HOST: &str = "login.dlsite.com";
const LOGIN_TWO_FACTOR_PATH: &str = "/login/two-factor";
/// Name of the verification-code input on the DLsite two-factor challenge form.
const TWO_FACTOR_CODE_FIELD: &str = "otp";
const LOGIN_PAGE_BODY_LIMIT: usize = 512 * 1024;
const LOGIN_SKIP_URL: &str = "https://www.dlsite.com/home/login/=/skip_register/1";
const LOGIN_FINISH_URL: &str = "https://www.dlsite.com/home/login/finish";

impl DlsiteClient {
    /// Logs in and returns the resulting session.
    ///
    /// Accounts with two-factor authentication enabled cannot complete in one step; they
    /// fail with [`ApiError::TwoFactorRequired`]. Use [`Self::begin_login`] together with
    /// [`Self::complete_two_factor`] to support them.
    pub async fn login(&self, credentials: &Credentials) -> Result<SessionSnapshot> {
        match self.begin_login(credentials).await? {
            LoginOutcome::Authorized(session) => Ok(session),
            LoginOutcome::TwoFactorRequired(_) => Err(ApiError::TwoFactorRequired),
        }
    }

    /// Submits login credentials.
    ///
    /// DLsite answers a two-factor-enabled account by redirecting to a challenge page rather
    /// than by failing, so a correct password can legitimately yield
    /// [`LoginOutcome::TwoFactorRequired`]. The returned challenge is bound to this client's
    /// cookie jar and must be completed through the same client.
    pub async fn begin_login(&self, credentials: &Credentials) -> Result<LoginOutcome> {
        self.http
            .get(LOGIN_URL)
            .query(&[("user", "self")])
            .diagnostic_send()
            .await?
            .error_for_status()?;

        let xsrf_token = self.xsrf_token()?;

        let auth_res = self
            .http
            .post(LOGIN_URL)
            .form(&[
                ("login_id", credentials.username.as_str()),
                ("password", credentials.password.as_str()),
                ("_token", xsrf_token.as_str()),
            ])
            .diagnostic_send()
            .await?;

        if auth_res.status() != StatusCode::FOUND {
            let endpoint = auth_res.url().clone();
            let status = auth_res.status();
            let body_snippet = response_text_snippet(auth_res).await;

            if body_snippet
                .as_deref()
                .is_some_and(|body| body.contains("ログインIDかパスワードが間違っています。"))
            {
                return Err(ApiError::InvalidCredentials);
            }

            return Err(ApiError::UnexpectedStatus {
                endpoint,
                status,
                body_snippet,
            });
        }

        // A wrong password also redirects, back to the login page, so the redirect target is
        // what distinguishes a two-factor challenge from an ordinary login result.
        if let Some(location) = redirect_location(&auth_res)
            .ok()
            .filter(is_two_factor_location)
        {
            let challenge = self.two_factor_challenge(location).await?;
            return Ok(LoginOutcome::TwoFactorRequired(challenge));
        }

        self.finish_login().await.map(LoginOutcome::Authorized)
    }

    /// Replays the two-factor challenge form with `code` filled in and finishes the login.
    ///
    /// A rejected code is reported as [`ApiError::InvalidTwoFactorCode`] so the caller can
    /// ask for another one instead of failing the whole operation.
    pub async fn complete_two_factor(
        &self,
        challenge: &TwoFactorChallenge,
        code: &str,
    ) -> Result<SessionSnapshot> {
        let mut form = challenge.fields.clone();
        form.push((challenge.code_field.clone(), code.trim().to_owned()));

        let res = self
            .http
            .post(challenge.action.clone())
            .form(&form)
            .diagnostic_send()
            .await?;
        let status = res.status();

        if status.is_redirection() {
            if redirect_location(&res).is_ok_and(|location| is_two_factor_location(&location)) {
                return Err(ApiError::InvalidTwoFactorCode);
            }

            return self.finish_login().await;
        }

        let endpoint = res.url().clone();

        if status == StatusCode::OK {
            let body = res.text().await?;

            // A rejected code re-renders the same challenge form with an error message.
            if parse_two_factor_challenge(&endpoint, &body).is_some() {
                return Err(ApiError::InvalidTwoFactorCode);
            }

            return Err(ApiError::UnexpectedStatus {
                endpoint,
                status,
                body_snippet: Some(text_snippet(&body)),
            });
        }

        let body_snippet = response_text_snippet(res).await;

        Err(ApiError::UnexpectedStatus {
            endpoint,
            status,
            body_snippet,
        })
    }

    async fn two_factor_challenge(&self, page_url: Url) -> Result<TwoFactorChallenge> {
        let raw = self
            .raw_get_with_body_limit(page_url.clone(), LOGIN_PAGE_BODY_LIMIT)
            .await?;

        if !(200..=299).contains(&raw.status) {
            return Err(ApiError::UnexpectedStatus {
                endpoint: raw.url,
                status: StatusCode::from_u16(raw.status)
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                body_snippet: raw.body_snippet,
            });
        }

        let body = raw.body_snippet.as_deref().unwrap_or_default();

        parse_two_factor_challenge(&raw.url, body).ok_or_else(|| {
            ApiError::TwoFactorFormNotRecognized {
                page: raw.url.clone(),
                body_snippet: raw.body_snippet.as_deref().map(text_snippet),
            }
        })
    }

    /// Runs the post-credential redirect dance that turns an authenticated login cookie into
    /// a usable Play session. Shared by the ordinary and two-factor login paths.
    async fn finish_login(&self) -> Result<SessionSnapshot> {
        let login_res = self.http.get(LOGIN_URL).diagnostic_send().await?;
        let login_res_status = login_res.status();
        let login_res_endpoint = login_res.url().clone();
        let login_res_body = response_text_snippet(login_res).await;

        if login_res_body
            .as_deref()
            .is_some_and(|body| body.contains("ログインIDかパスワードが間違っています。"))
        {
            return Err(ApiError::InvalidCredentials);
        }

        if login_res_status != StatusCode::OK && login_res_status != StatusCode::FOUND {
            return Err(ApiError::UnexpectedStatus {
                endpoint: login_res_endpoint,
                status: login_res_status,
                body_snippet: login_res_body,
            });
        }

        let skip_location = self.redirect_location_from_get(LOGIN_SKIP_URL).await?;
        let oauth_request_location = self.redirect_location_from_get(skip_location).await?;

        self.redirect_location_from_get(oauth_request_location)
            .await?;

        self.http
            .get(LOGIN_FINISH_URL)
            .diagnostic_send()
            .await?
            .error_for_status()?;

        match self.validate_session().await? {
            SessionStatus::Authorized => self.export_session(),
            SessionStatus::Unauthorized => Err(ApiError::InvalidCredentials),
        }
    }

    pub fn export_session(&self) -> Result<SessionSnapshot> {
        let mut writer = BufWriter::new(Vec::new());
        let guard = self
            .cookie_store
            .lock()
            .map_err(|_| ApiError::CookieStore("cookie store mutex is poisoned".to_owned()))?;

        cookie_store::serde::json::save(&guard, &mut writer)
            .map_err(|err| ApiError::CookieStore(format!("{err:?}")))?;

        drop(guard);

        let bytes = writer
            .into_inner()
            .map_err(|err| ApiError::CookieStore(format!("{err:?}")))?;
        let cookies_json =
            String::from_utf8(bytes).map_err(|err| ApiError::CookieStore(format!("{err:?}")))?;

        Ok(SessionSnapshot { cookies_json })
    }

    pub fn import_session(&self, snapshot: &SessionSnapshot) -> Result<()> {
        let parsed = cookie_store::serde::json::load(snapshot.cookies_json.as_bytes())
            .map_err(|err| ApiError::CookieStore(format!("{err:?}")))?;
        let mut guard = self
            .cookie_store
            .lock()
            .map_err(|_| ApiError::CookieStore("cookie store mutex is poisoned".to_owned()))?;

        *guard = parsed;
        Ok(())
    }

    pub async fn validate_session(&self) -> Result<SessionStatus> {
        let res = self.http.get(CONTENT_COUNT_URL).diagnostic_send().await?;

        match res.status() {
            StatusCode::OK => {
                let endpoint = res.url().clone();
                let body = res.text().await?;
                let count = parse_json_body::<ContentCount>(endpoint, &body)?;
                self.cache_limits_from_count(&count).await;
                Ok(SessionStatus::Authorized)
            }
            StatusCode::UNAUTHORIZED => Ok(SessionStatus::Unauthorized),
            status => {
                let endpoint = res.url().clone();
                let body_snippet = response_text_snippet(res).await;
                Err(ApiError::UnexpectedStatus {
                    endpoint,
                    status,
                    body_snippet,
                })
            }
        }
    }

    /// Submits the login form the same way [`Self::login`] does, but returns the raw
    /// credential-POST response instead of continuing the post-login redirect chain.
    ///
    /// This exists so the two-factor challenge page can be observed as-is; DLsite does not
    /// document its two-factor form, so the concrete contract has to be probed live.
    pub async fn raw_login_probe(
        &self,
        credentials: &Credentials,
        body_limit: usize,
    ) -> Result<RawResponse> {
        self.http
            .get(LOGIN_URL)
            .query(&[("user", "self")])
            .diagnostic_send()
            .await?
            .error_for_status()?;

        let xsrf_token = self.xsrf_token()?;
        let res = self
            .http
            .post(LOGIN_URL)
            .form(&[
                ("login_id", credentials.username.as_str()),
                ("password", credentials.password.as_str()),
                ("_token", xsrf_token.as_str()),
            ])
            .diagnostic_send()
            .await?;

        RawResponse::from_response_with_body_limit(res, body_limit).await
    }

    fn xsrf_token(&self) -> Result<String> {
        let guard = self
            .cookie_store
            .lock()
            .map_err(|_| ApiError::CookieStore("cookie store mutex is poisoned".to_owned()))?;

        guard
            .get("login.dlsite.com", "/", "XSRF-TOKEN")
            .map(|cookie| cookie.value().to_owned())
            .ok_or(ApiError::XsrfTokenNotFound)
    }

    async fn redirect_location_from_get(&self, url: impl reqwest::IntoUrl) -> Result<Url> {
        let res = self.http.get(url).diagnostic_send().await?;
        let status = res.status();

        if !status.is_redirection() {
            let endpoint = res.url().clone();
            let body_snippet = response_text_snippet(res).await;
            return Err(ApiError::UnexpectedStatus {
                endpoint,
                status,
                body_snippet,
            });
        }

        redirect_location(&res)
    }
}

pub(super) fn is_two_factor_location(url: &Url) -> bool {
    url.host_str() == Some(LOGIN_HOST) && url.path().trim_end_matches('/') == LOGIN_TWO_FACTOR_PATH
}

pub(super) fn parse_two_factor_challenge(page_url: &Url, body: &str) -> Option<TwoFactorChallenge> {
    let (open_tag, inner) = find_form_containing(body, TWO_FACTOR_CODE_FIELD)?;
    let attributes = parse_html_attributes(open_tag);
    let action = match html_attribute(&attributes, "action") {
        Some(action) => page_url.join(&decode_basic_html_entities(action)).ok()?,
        None => page_url.clone(),
    };

    Some(TwoFactorChallenge {
        page_url: page_url.clone(),
        action,
        code_field: TWO_FACTOR_CODE_FIELD.to_owned(),
        fields: parse_form_fields(inner, TWO_FACTOR_CODE_FIELD),
    })
}

/// Returns the open tag and inner HTML of the first `<form>` holding a control named
/// `control_name`.
fn find_form_containing<'a>(body: &'a str, control_name: &str) -> Option<(&'a str, &'a str)> {
    let mut rest = body;

    loop {
        let start = rest.find("<form")?;
        rest = &rest[start..];

        let open_end = rest.find('>')?;
        let close = rest.find("</form>")?;

        if close < open_end {
            return None;
        }

        let open_tag = &rest[..=open_end];
        let inner = &rest[open_end + 1..close];
        let has_control = form_control_tags(inner)
            .iter()
            .any(|attributes| html_attribute(attributes, "name") == Some(control_name));

        if has_control {
            return Some((open_tag, inner));
        }

        rest = &rest[close + "</form>".len()..];
    }
}

/// Collects the fields a browser would submit for this form, excluding the code field which
/// the caller supplies. Values are kept as rendered so per-request tokens survive the replay.
pub(super) fn parse_form_fields(inner: &str, code_field: &str) -> Vec<(String, String)> {
    let mut fields = Vec::new();

    for attributes in form_control_tags(inner) {
        let Some(name) = html_attribute(&attributes, "name") else {
            continue;
        };

        if name.is_empty() || name == code_field {
            continue;
        }

        let kind = html_attribute(&attributes, "type")
            .unwrap_or("text")
            .to_ascii_lowercase();

        match kind.as_str() {
            // Browsers only submit checked toggles, and never submit buttons or file inputs.
            "checkbox" | "radio" if !has_html_attribute(&attributes, "checked") => continue,
            "submit" | "button" | "reset" | "image" | "file" => continue,
            _ => {}
        }

        let value = html_attribute(&attributes, "value").unwrap_or_default();

        fields.push((name.to_owned(), decode_basic_html_entities(value)));
    }

    fields
}

/// Yields the parsed attributes of every form-control open tag, in document order.
fn form_control_tags(inner: &str) -> Vec<Vec<(String, String)>> {
    const CONTROL_TAGS: &[&str] = &["input", "select", "textarea"];

    let mut controls = Vec::new();
    let mut rest = inner;

    while let Some(start) = rest.find('<') {
        rest = &rest[start..];

        let Some(open_end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..=open_end];
        let name_end = tag[1..]
            .find(|value: char| value.is_whitespace() || value == '>' || value == '/')
            .map_or(tag.len(), |index| index + 1);
        let tag_name = tag[1..name_end].to_ascii_lowercase();

        if CONTROL_TAGS.contains(&tag_name.as_str()) {
            controls.push(parse_html_attributes(tag));
        }

        rest = &rest[open_end + 1..];
    }

    controls
}

fn html_attribute<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn has_html_attribute(attributes: &[(String, String)], name: &str) -> bool {
    attributes.iter().any(|(key, _)| key == name)
}

fn parse_html_attributes(tag: &str) -> Vec<(String, String)> {
    let chars = tag.chars().collect::<Vec<_>>();
    let mut attributes = Vec::new();
    let mut index = 0;

    while index < chars.len() && !chars[index].is_whitespace() {
        index += 1;
    }

    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }

        if index >= chars.len() || chars[index] == '>' || chars[index] == '/' {
            break;
        }

        let name_start = index;

        while index < chars.len()
            && !chars[index].is_whitespace()
            && chars[index] != '='
            && chars[index] != '>'
        {
            index += 1;
        }

        let name = chars[name_start..index]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        let mut value = String::new();

        if index < chars.len() && chars[index] == '=' {
            index += 1;

            if index < chars.len() && (chars[index] == '"' || chars[index] == '\'') {
                let quote = chars[index];
                index += 1;

                let value_start = index;

                while index < chars.len() && chars[index] != quote {
                    index += 1;
                }

                value = chars[value_start..index].iter().collect();

                if index < chars.len() {
                    index += 1;
                }
            } else {
                let value_start = index;

                while index < chars.len() && !chars[index].is_whitespace() && chars[index] != '>' {
                    index += 1;
                }

                value = chars[value_start..index].iter().collect();
            }
        }

        if !name.is_empty() {
            attributes.push((name, value));
        }
    }

    attributes
}
