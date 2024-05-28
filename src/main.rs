use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use dotenv::dotenv;
use log::{error, info};
use reqwest::{
    header::{HeaderMap, AUTHORIZATION, CONTENT_TYPE},
    StatusCode,
};
use serde::Deserialize;
use serde_json::{json, Value};
use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    slots_idle: Option<u32>,
    slots_processing: Option<u32>,
}

const URL: &str = "http://192.168.2.56:8080";

#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init();
    log::info!("Starting command bot...");

    let bot = Bot::from_env();

    // Get the bot commands
    bot.set_my_commands(Command::bot_commands()).await.unwrap();

    info!(
        "{} has started!",
        bot.get_me().send().await.unwrap().user.username.unwrap()
    );

    Command::repl(bot, answer).await;
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "This bot is 100% hosted on a 512MB Raspberry Pi Zero 2 W. Expect low performance and low quality.\n\nThese commands are supported:"
)]
enum Command {
    #[command(description = "LLM request")]
    Qwen(String),
    #[command(description = "Prints this help")]
    Help,
    #[command(description = "Health check")]
    Health,
}

async fn answer(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .reply_to_message_id(msg.id)
                .await?
        }
        Command::Qwen(prompt) => {
            info!("Received LLM request: {}", prompt);
            let url = format!("{}/v1/chat/completions", URL);

            // Create headers
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
            headers.insert(AUTHORIZATION, "Bearer amogus".parse().unwrap());

            // Create the body
            let body = json!({
                "model": "amogus", // model doesn't matter, llama.cpp uses qwen 0.5b under the hood
                "messages": [
                    {
                        "role": "user",
                        "content": prompt
                    }
                ],
                "temperature": 0.4, // low temperature because this model is so small any variation will probably be bad
            });

            // Send the request
            let client = reqwest::Client::new();

            // Before we send the request, send the typing indicator every 5 seconds in a different thread
            let flag = Arc::new(AtomicBool::new(false));
            let flag_clone = Arc::clone(&flag);

            let bot_clone = bot.clone();
            let msg_clone = msg.clone();
            tokio::spawn(async move {
                loop {
                    if flag_clone.load(Ordering::Relaxed) {
                        info!("Stopping typing indicator");
                        break;
                    }
                    info!("Sending typing indicator...");
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    bot_clone
                        .send_chat_action(msg_clone.chat.id, teloxide::types::ChatAction::Typing)
                        .await
                        .unwrap();
                }
            });

            info!("Sending request to {}", url);
            let now = std::time::Instant::now();
            let res = client.post(&url).headers(headers).json(&body).send().await;
            info!("Request took {}ms", now.elapsed().as_millis());
            // Stop the typing indicator
            flag.store(true, Ordering::Relaxed);
            // There is probably a better way to do this but this works for now

            let res = match res {
                Ok(res) => res,
                Err(e) => {
                    error!("Error sending request: {}", e);
                    bot.send_message(msg.chat.id, "An error occurred while sending the request.")
                        .reply_to_message_id(msg.id)
                        .await?;
                    return Ok(());
                }
            };

            // Parse the response
            let res_text = res.text().await;
            let res_text = match res_text {
                Ok(res_text) => res_text,
                Err(e) => {
                    error!("Error reading response: {}", e);
                    bot.send_message(msg.chat.id, "An error occurred while reading the response.")
                        .reply_to_message_id(msg.id)
                        .await?;
                    return Ok(());
                }
            };
            let parsed_response = serde_json::from_str::<Value>(&res_text);
            let parsed_response = match parsed_response {
                Ok(parsed_response) => parsed_response,
                Err(e) => {
                    error!("Error parsing response: {}", e);
                    bot.send_message(msg.chat.id, "An error occurred while parsing the response.")
                        .reply_to_message_id(msg.id)
                        .await?;
                    return Ok(());
                }
            };

            let response = match parsed_response["choices"][0]["message"]["content"].as_str() {
                Some(response) => response,
                None => {
                    error!("Error parsing response: {:?}", parsed_response);
                    bot.send_message(msg.chat.id, "An error occurred while parsing the response.")
                        .reply_to_message_id(msg.id)
                        .await?;
                    return Ok(());
                }
            };

            info!("Response: {}", response);
            bot.send_message(msg.chat.id, response)
                .reply_to_message_id(msg.id)
                .await?
        }
        Command::Health => {
            info!("Received health check request");
            let response = reqwest::get(&format!("{}/health", URL)).await;
            let response = match response {
                Ok(response) => response,
                Err(e) => {
                    error!("Error sending health check request: {}", e);
                    bot.send_message(
                        msg.chat.id,
                        "An error occurred while sending the health check request.",
                    )
                    .reply_to_message_id(msg.id)
                    .await?;
                    return Ok(());
                }
            };
            let status = response.status();
            let body = response.text().await;
            let body = match body {
                Ok(body) => {
                    info!("Health check response: {}", body);
                    body
                }
                Err(e) => {
                    error!("Error reading health check response: {}", e);
                    bot.send_message(
                        msg.chat.id,
                        "An error occurred while reading the health check response.",
                    )
                    .reply_to_message_id(msg.id)
                    .await?;
                    return Ok(());
                }
            };

            let message = match status {
                StatusCode::OK => {
                    let health: HealthResponse = serde_json::from_str(&body).unwrap();
                    match health.status.as_str() {
                        "ok" => format!("Everything is working fine. Slots idle: {}, Slots processing: {}", health.slots_idle.unwrap_or(0), health.slots_processing.unwrap_or(0)),
                        "no slot available" => format!("No slots are currently available. Slots idle: {}, Slots processing: {}", health.slots_idle.unwrap_or(0), health.slots_processing.unwrap_or(0)),
                        _ => format!("Unknown status: {}", health.status),
                    }
                }
                StatusCode::SERVICE_UNAVAILABLE => {
                    let health: HealthResponse = serde_json::from_str(&body).unwrap();
                    match health.status.as_str() {
                        "loading model" => "The model is still being loaded. Please wait.".to_string(),
                        "no slot available" => format!("No slots are currently available. Slots idle: {}, Slots processing: {}", health.slots_idle.unwrap_or(0), health.slots_processing.unwrap_or(0)),
                        _ => format!("Unknown status: {}", health.status),
                    }
                }
                StatusCode::INTERNAL_SERVER_ERROR => {
                    let health: HealthResponse = serde_json::from_str(&body).unwrap();
                    match health.status.as_str() {
                        "error" => "An error occurred while loading the model.".to_string(),
                        _ => format!("Unknown status: {}", health.status),
                    }
                }
                _ => format!("Unexpected status: {}", status),
            };

            info!("Health check response: {}", message);
            bot.send_message(msg.chat.id, message)
                .reply_to_message_id(msg.id)
                .await?
        }
    };

    Ok(())
}
