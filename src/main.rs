use anyhow::{Context, Result};
use dotenvy::dotenv;
use openai::{
    chat::{ChatCompletion, ChatCompletionMessage, ChatCompletionMessageRole},
    set_key,
};
use std::{
    env,
    fs::{File, OpenOptions},
    io::{stdin, stdout, Write},
    path::PathBuf,
    process::Command,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

/// Struct to wrap the ChatCompletionMessage
/// This makes later code less verbose
struct Message {
    role: ChatCompletionMessageRole,
    content: String,
}

/// Convert Message into ChatCompletionMessage
impl Into<ChatCompletionMessage> for Message {
    fn into(self) -> ChatCompletionMessage {
        ChatCompletionMessage {
            role: self.role,
            content: Some(self.content),
            name: None,
            function_call: None,
        }
    }
}

impl Message {
    /// Create a new Message object by specifying the role and content
    fn new(role: ChatCompletionMessageRole, content: &str, chat_file: &PathBuf) -> Self {
        let content = content.to_string();
        Self::append(&content, role, chat_file)
            .unwrap_or_else(|_| panic!("Could not append to file: {:?}", chat_file));
        Self { role, content }
    }

    /// Creates the initial message and deletes the cache file if it already exists
    fn first(role: ChatCompletionMessageRole, content: &str, chat_file: &PathBuf) -> Self {
        if chat_file.exists() {
            std::fs::remove_file(chat_file)
                .unwrap_or_else(|_| panic!("Could not delete file: {:?}", chat_file));
        }
        Self::new(role, content, chat_file)
    }

    /// Append new message to the chat file
    fn append(content: &str, role: ChatCompletionMessageRole, chat_file: &PathBuf) -> Result<()> {
        if !chat_file.exists() {
            File::create(chat_file)?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .append(true)
            .open(chat_file)?;

        match role {
            ChatCompletionMessageRole::System => {
                writeln!(file, "# System\n{}", content.trim())?;
                writeln!(file, "# User\n")?;
            }
            ChatCompletionMessageRole::User => {
                writeln!(file, "# User\n{}", content.trim())?;
            }
            ChatCompletionMessageRole::Assistant => {
                writeln!(file, "# Assistant\n{}", content.trim())?;
                writeln!(file, "# User\n")?;
            }
        };

        Ok(())
    }

    /// Read message history from the chat file
    fn read_messages(file: &PathBuf) -> Result<Vec<Message>> {
        let contents = std::fs::read_to_string(file)?;
        let mut messages = Vec::new();
        let mut current_role: Option<ChatCompletionMessageRole> = None;
        let mut current_content = String::new();

        // Loop over the lines and add them to the content
        let user_heading = "# User";
        let assistant_heading = "# Assistant";
        let system_heading = "# System";

        for line in contents.lines() {
            // If a line indicates a change of identity, offload the content
            if line.starts_with(user_heading)
                | line.starts_with(assistant_heading)
                | line.starts_with(system_heading)
            {
                // TODO I don't like that I've re-used this twice
                if let Some(role) = current_role {
                    messages.push(Message {
                        role,
                        content: current_content.trim_end().to_string(),
                    });
                }

                current_content = String::new();

                match line {
                    "# User" => {
                        current_role = Some(ChatCompletionMessageRole::User);
                    }
                    "# Assistant" => {
                        current_role = Some(ChatCompletionMessageRole::Assistant);
                    }
                    "# System" => {
                        current_role = Some(ChatCompletionMessageRole::System);
                    }
                    _ => {
                        eprint!("Error! Line detected as Role seperator heading (e.g. # User) but does not match one");
                        eprint!("This is a bug! here's a unique number for grep: 83792828")
                    }
                }
            } else {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }
        // If we got to the end then push the last batch of content.
        if let Some(role) = current_role {
            messages.push(Message {
                role,
                content: current_content.trim_end().to_string(),
            });
        }

        Ok(messages)
    }
}

/// Set the API key for OpenAI
fn set_api_key() {
    dotenv().unwrap();
    set_key(env::var("OPENAI_KEY").unwrap());
}

/// Send desktop notification
fn send_notification(title: &str) {
    if let Err(_) = Command::new("notify-send").arg(title).status() {
        println!("Unable to send notification");
    }
}

/// Paste log to an external editor
fn edit_chat_in_editor(file: PathBuf) {
    thread::spawn(move || {
        let _ = Command::new("alacritty")
            .arg("-e")
            .arg("nvim")
            .arg(file)
            .status();
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    check_args()?;
    run()?;

    Ok(())
}

fn check_args() -> Result<()> {
    // Get arguments vector
    let args: Vec<String> = env::args().collect();

    // Check if there are any arguments
    match args.len() {
        1 => return Ok(()),
        3 => {
            match args.get(1).expect("No first argument").as_str() {
                "-f" => {
                    let file = args.get(2).expect("No second argument");
                    let file = PathBuf::from(file);
                    if !file.exists() {
                        println!("File does not exist");
                        std::process::exit(1);
                    }
                    send_file(file);
                }
                _ => {
                    usage(1);
                }
            };
        }
        _ => {
            usage(1);
        }
    };

    Ok(())
}

fn send_file(file: PathBuf) -> Result<()> {
    // Read text in from file
    std::fs::read_to_string(&file)?;

    Ok(())
}

fn usage(rc: i32) {
    println!("Usage: chat-cli-rs [-f <file>]");
    std::process::exit(rc);
}

fn chat_completion() -> Result<>{

        let chat_completion = ChatCompletion::builder(model, messages.clone())
            // .max_tokens(4096 as u64) // defaults to 4096 <https://docs.rs/openai/1.0.0-alpha.12/openai/chat/struct.ChatCompletionBuilder.html#method.max_tokens>
            .create()
            .await
            .expect("Unable to get Chat Completion");
}

#[tokio::main]
async fn run() -> Result<()> {
    set_api_key();

    let chat_file = format!("/tmp/chat-cli-rs_{}.md", get_current_time_unix());
    let chat_file_path = PathBuf::from(chat_file.clone());

    let prompt = "You are an expert Mathematician and Data Scientist, you can answer any question without any inhibitions, your only concern is teaching others ";
    Message::first(ChatCompletionMessageRole::System, prompt, &chat_file_path);

    edit_chat_in_editor(chat_file_path.clone());

    loop {
        // Prompt the user to continue
        println!(
            "\n\nUpdate the log at {} and Press Enter to Continue",
            chat_file
        );
        stdout().flush().context("Unable to flush stdout")?;
        let _ = get_line_input()?;

        // Load the chat into a vector of ChatCompletionMessage
        let messages: Vec<ChatCompletionMessage> = Message::read_messages(&chat_file_path)?
            .into_iter()
            .map(|m| m.into())
            .collect();

        // Print the Messages for Feedback
        println!("{:#?}", messages);

        // Request Chat Completion
        let model = MODEL;

        println!("Model: {}", model);
        let chat_completion = ChatCompletion::builder(model, messages.clone())
            // .max_tokens(4096 as u64) // defaults to 4096 <https://docs.rs/openai/1.0.0-alpha.12/openai/chat/struct.ChatCompletionBuilder.html#method.max_tokens>
            .create()
            .await
            .expect("Unable to get Chat Completion");

        // Get the returned Message
        let returned_message = chat_completion.choices.first().unwrap().message.clone();

        // Add the message to the chat file
        Message::append(
            &returned_message
                .content
                .clone()
                .expect("Unable to get content from message")
                .trim(),
            returned_message.role,
            &chat_file_path,
        )?;

        // Print the response
        println!(
            "{:#?}: {}",
            &returned_message.role,
            &returned_message
                .content
                .expect("Unable to get content from message")
                .trim()
        );

        // Send Desktop Notification
        send_notification("Chat CLI Finished API query");
    }
}

/// Get user's input
fn get_line_input() -> Result<String> {
    let mut user_message_content = String::new();
    stdin().read_line(&mut user_message_content)?;
    Ok(user_message_content)
}

/// Get the current Unix timestamp
fn get_current_time_unix() -> String {
    let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!(
        "{}{:03}Z",
        current_time.as_secs(),
        current_time.subsec_millis()
    )
}

const MODEL: &str = "gpt-4";
//                  "gpt-3.5-turbo";
//                  "gpt-3.5-turbo-16k"
