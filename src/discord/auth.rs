//! Signing in: email and password, the two-factor code, and checking a
//! token. Never log a password, ticket or token.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{API, Me};

/// What the login form gets back.
#[derive(Debug)]
pub enum Outcome {
    Token(String),
    /// The account has two-factor authentication; the code goes with this ticket.
    Mfa {
        ticket: String,
    },
}

#[derive(Debug, Clone)]
pub enum Error {
    /// Shown beside the field Discord blamed.
    Fields {
        login: Option<String>,
        password: Option<String>,
        code: Option<String>,
    },
    /// Discord wants an hCaptcha, which only a browser can show.
    Captcha,
    RateLimited {
        seconds: f32,
    },
    Network,
    Other(String),
}

impl Error {
    /// A sentence for the screen.
    pub fn message(&self) -> String {
        match self {
            Error::Fields {
                login,
                password,
                code,
            } => login
                .clone()
                .or(password.clone())
                .or(code.clone())
                .unwrap_or_default(),
            Error::Captcha => {
                "O Discord pediu um captcha para este login. Entre pelo código QR ou com um token."
                    .into()
            }
            Error::RateLimited { seconds } => {
                format!(
                    "Muitas tentativas. Tente de novo em {} segundos.",
                    seconds.ceil() as u32
                )
            }
            Error::Network => "Não foi possível falar com o Discord. Verifique sua conexão.".into(),
            Error::Other(message) => message.clone(),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        log::warn!("Discord request failed: {error}");
        Error::Network
    }
}

pub async fn login(
    client: &reqwest::Client,
    login: &str,
    password: &str,
) -> Result<Outcome, Error> {
    let body = json!({
        "login": login,
        "password": password,
        "undelete": false,
        "login_source": null,
        "gift_code_sku_id": null,
    });
    let value = post(client, "/auth/login", &body).await?;
    if value.get("mfa").and_then(Value::as_bool) == Some(true) {
        let ticket = value["ticket"]
            .as_str()
            .ok_or_else(|| Error::Other("Resposta inesperada do Discord.".into()))?;
        return Ok(Outcome::Mfa {
            ticket: ticket.to_owned(),
        });
    }
    token_of(&value).map(Outcome::Token)
}

/// The six-digit code from an authenticator app, or an eight-character
/// backup code.
pub async fn mfa(client: &reqwest::Client, ticket: &str, code: &str) -> Result<String, Error> {
    let code: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    let body =
        json!({ "code": code, "ticket": ticket, "login_source": null, "gift_code_sku_id": null });
    let value = post(client, "/auth/mfa/totp", &body)
        .await
        .map_err(|error| match error {
            Error::Other(message) => Error::Fields {
                login: None,
                password: None,
                code: Some(message),
            },
            other => other,
        })?;
    token_of(&value)
}

/// Asks Discord to email a password reset link.
pub async fn forgot(client: &reqwest::Client, login: &str) -> Result<(), Error> {
    post(client, "/auth/forgot", &json!({ "login": login }))
        .await
        .map(|_| ())
}

/// Trades the ticket the phone approved for the encrypted token.
pub async fn remote_login(client: &reqwest::Client, ticket: &str) -> Result<String, Error> {
    let value = post(
        client,
        "/users/@me/remote-auth/login",
        &json!({ "ticket": ticket }),
    )
    .await?;
    value["encrypted_token"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::Other("Resposta inesperada do Discord.".into()))
}

/// Who a token belongs to. `Ok(None)` when Discord no longer accepts it.
pub async fn me(client: &reqwest::Client, token: &str) -> Result<Option<Me>, Error> {
    let response = client
        .get(format!("{API}/users/@me"))
        .header("Authorization", token)
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(Error::Other(format!(
            "O Discord respondeu {}.",
            response.status()
        )));
    }
    Ok(Some(response.json().await?))
}

/// Ends the session on Discord's side too.
pub async fn logout(client: &reqwest::Client, token: &str) {
    let _ = client
        .post(format!("{API}/auth/logout"))
        .header("Authorization", token)
        .json(&json!({ "provider": null, "voip_provider": null }))
        .send()
        .await;
}

fn token_of(value: &Value) -> Result<String, Error> {
    value["token"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::Other("Resposta inesperada do Discord.".into()))
}

async fn post(client: &reqwest::Client, path: &str, body: &Value) -> Result<Value, Error> {
    let response = client
        .post(format!("{API}{path}"))
        .header("X-Discord-Locale", "pt-BR")
        .json(body)
        .send()
        .await?;
    let status = response.status();
    let value: Value = response.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(value);
    }
    Err(error_of(status, &value))
}

#[derive(Deserialize)]
struct FieldError {
    code: String,
    message: String,
}

fn error_of(status: reqwest::StatusCode, value: &Value) -> Error {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let seconds = value["retry_after"].as_f64().unwrap_or(5.0) as f32;
        return Error::RateLimited { seconds };
    }
    if value.get("captcha_key").is_some() {
        return Error::Captcha;
    }
    let field = |name: &str| -> Option<String> {
        let errors = value.get("errors")?.get(name)?.get("_errors")?;
        let first: FieldError = serde_json::from_value(errors.get(0)?.clone()).ok()?;
        Some(translate(&first.code).map_or(first.message, str::to_owned))
    };
    let (login, password, code) = (
        field("login").or(field("email")),
        field("password"),
        field("code"),
    );
    if login.is_some() || password.is_some() || code.is_some() {
        return Error::Fields {
            login,
            password,
            code,
        };
    }
    match value["code"].as_i64() {
        // Invalid two-factor code, or an expired ticket.
        Some(60008) => Error::Other("Código de autenticação inválido.".into()),
        Some(60006) | Some(60009) => {
            Error::Other("Sua sessão de login expirou. Volte e entre de novo.".into())
        }
        _ => Error::Other(
            value["message"]
                .as_str()
                .map_or_else(|| format!("O Discord respondeu {status}."), str::to_owned),
        ),
    }
}

fn translate(code: &str) -> Option<&'static str> {
    Some(match code {
        "INVALID_LOGIN" => "Login ou senha inválidos.",
        "BASE_TYPE_REQUIRED" => "Este campo é obrigatório",
        "EMAIL_TYPE_INVALID_EMAIL" => "Não é um endereço de e-mail válido.",
        "ACCOUNT_LOGIN_VERIFICATION_EMAIL" => {
            "Novo local de login detectado. Verifique seu e-mail."
        }
        "ACCOUNT_COMPROMISED_RESET_PASSWORD" => "Redefina sua senha para entrar.",
        _ => return None,
    })
}
