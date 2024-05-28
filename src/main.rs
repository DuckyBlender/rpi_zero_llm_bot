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

    Command::repl(bot, answer).await;
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
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
                .await?
        }
        Command::Qwen(prompt) => {
            info!("Received LLM request: {}", prompt);
            let url = format!("{}/v1/chat/completions", URL);

            // Create headers
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
            headers.insert(AUTHORIZATION, "Bearer no-key".parse().unwrap());

            // Create the body
            let body = json!({
                "model": "gpt-3.5-turbo", // model doesn't matter, llama.cpp uses qwen 0.5b under the hood
                "messages": [
                    {
                        "role": "user",
                        "content": prompt
                    }
                ],
                "temperature": 0.4, // low temperature because this model is so small any variation will probably be bad
            });

            // Send the request
            info!("Sending request to {}", url);
            let client = reqwest::Client::new();
            let res = client.post(&url).headers(headers).json(&body).send().await;

            let res = match res {
                Ok(res) => res,
                Err(e) => {
                    error!("Error sending request: {}", e);
                    bot.send_message(msg.chat.id, "An error occurred while sending the request.")
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
                        .await?;
                    return Ok(());
                }
            };

            // Now you can access the fields in the parsed_response
            println!("Response ID: {}", parsed_response["id"]);

            let response = match parsed_response["choices"][0]["message"]["content"].as_str() {
                Some(response) => response,
                None => {
                    error!("Error parsing response: {:?}", parsed_response);
                    bot.send_message(msg.chat.id, "An error occurred while parsing the response.")
                        .await?;
                    return Ok(());
                }
            };

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
