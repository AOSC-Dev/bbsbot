use std::{env, sync::Arc};

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use eyre::OptionExt;
use serde::Deserialize;
use teloxide::{
    payloads::SendMessageSetters,
    requests::Requester,
    types::{ChatId, ParseMode},
    Bot,
};
use tokio::fs;
use tracing::{error, level_filters::LevelFilter};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

#[derive(Debug, Deserialize)]
struct Config {
    webhook: String,
    send_telegram_ids: Vec<i64>,
    token: String,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    dotenvy::dotenv().ok();

    let path = env::args().nth(1).ok_or_eyre("Arg path is not set")?;
    let env_log = EnvFilter::try_from_default_env();
    let config = fs::read_to_string(path).await?;
    let config: Config = toml::from_str(&config)?;

    let bot = Bot::new(&config.token);

    if let Ok(filter) = env_log {
        tracing_subscriber::registry()
            .with(
                fmt::layer()
                    .event_format(
                        tracing_subscriber::fmt::format()
                            .with_file(true)
                            .with_line_number(true),
                    )
                    .with_filter(filter),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(
                fmt::layer()
                    .event_format(
                        tracing_subscriber::fmt::format()
                            .with_file(true)
                            .with_line_number(true),
                    )
                    .with_filter(LevelFilter::INFO),
            )
            .init();
    }

    let app = Router::new()
        .route("/", post(handler))
        .with_state((Arc::new(bot), config.send_telegram_ids));

    let listener = tokio::net::TcpListener::bind(config.webhook).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

struct EyreError {
    err: eyre::Error,
}

impl IntoResponse for EyreError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.err),
        )
            .into_response()
    }
}

impl<E> From<E> for EyreError
where
    E: Into<eyre::Error>,
{
    fn from(err: E) -> Self {
        EyreError { err: err.into() }
    }
}

#[derive(Deserialize)]
enum Event {
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "topic")]
    Topic { title: String, id: u64 },
}

async fn handler(
    State((bot, list)): State<(Arc<Bot>, Vec<i64>)>,
    Json(json): Json<Event>,
) -> Result<(), EyreError> {
    match json {
        Event::Ping { .. } => return Ok(()),
        Event::Topic { title, id } => {
            let title = Arc::new(title);
            tokio::spawn(async move {
                for i in list {
                    let botc = bot.clone();
                    let tc = title.clone();
                    tokio::spawn(async move {
                        let res = botc
                        .send_message(
                            ChatId(i),
                            format!(
                                "<b>AOSC BBS</b>\n<a href=\"https://bbs.aosc.io/t/topic/{}\">{}</a>",
                                id, tc.clone()
                            ),
                        )
                        .parse_mode(ParseMode::Html)
                        .disable_web_page_preview(true)
                        .await;

                        if let Err(e) = res {
                            error!("{e}");
                        }
                    });
                }
            });
        }
    }

    Ok(())
}
