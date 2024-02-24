use anyhow::{Context, Result};
use openai::{
    chat::{ChatCompletion, ChatCompletionDelta, ChatCompletionMessage, ChatCompletionMessageRole},
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
use tokio::sync::mpsc::Receiver;

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
            ChatCompletionMessageRole::Function => todo!("I'm not sure if this needs to become unimplemented, I haven't read this new feature"),
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
                        eprint!("This may be a bug! here's a unique number for grep: 83792828")
                        // could just be a top level heading maybe?
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
    // dotenv().unwrap();
    set_key(env::var("OPENAI_API_KEY").unwrap());
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
        //         let _ = Command::new("alacritty")
        //             .arg("-e")
        //             .arg("nvim")
        //             .arg(file)
        //             .status();

        // TODO we should be able to override this
        let _ = Command::new("Neovide.AppImage").arg(file).spawn();
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    // TODO this code is awful, rewrite from scratch for the -f
    // functions should be methods
    // share between -f and loop()
    set_api_key();
    // TODO Consider using clap to allow changing model
    check_args().await?;
    run().await?;

    Ok(())
}

async fn check_args() -> Result<()> {
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
                    send_file(file)
                        .await
                        .unwrap_or_else(|_| panic!("Unable to send file"));
                    std::process::exit(0);
                }
                "-h" | "--help" => {
                    usage(0);
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

async fn send_file(file: PathBuf) -> Result<()> {
    // Load the chat into a vector of ChatCompletionMessage
    let messages: Vec<ChatCompletionMessage> = Message::read_messages(&file)?
        .into_iter()
        .map(|m| m.into())
        .collect();

    // Print the Messages for Feedback
    println!("{:#?}", messages);

    let returned_message = match request_chat_completion(messages.clone()).await {
        Ok(m) => m,
        Err(e) => {
            panic!("Error: {:?}", e);
        }
    };

    append_message_to_file(returned_message, file)?;

    Ok(())
}

fn usage(rc: i32) {
    println!("Usage: chat-cli-rs [-f <file>]");
    std::process::exit(rc);
}

// TODO should this be a method?
// This is unused but exists as a simpler fall back method
async fn request_chat_completion_block_and_wait(
    messages: Vec<ChatCompletionMessage>,
) -> Result<ChatCompletionMessage> {
    // Request Chat Completion
    let model = MODEL;

    let chat_completion = ChatCompletion::builder(model, messages.clone())
        // .max_tokens(4096 as u64) // defaults to 4096 <https://docs.rs/openai/1.0.0-alpha.12/openai/chat/struct.ChatCompletionBuilder.html#method.max_tokens>
        .create()
        .await
        .expect("Unable to get Chat Completion");

    // Get the returned Message
    Ok(chat_completion.choices.first().unwrap().message.clone())
}

// TODO should this be a method?
async fn request_chat_completion(
    messages: Vec<ChatCompletionMessage>,
) -> Result<ChatCompletionMessage> {
    // Request Chat Completion
    let model = MODEL;

    let chat_stream = ChatCompletionDelta::builder(model, messages.clone())
        // .max_tokens(4096 as u64) // defaults to 4096 <https://docs.rs/openai/1.0.0-alpha.12/openai/chat/struct.ChatCompletionBuilder.html#method.max_tokens>
        .create_stream()
        .await
        .expect("Unable to get Chat Stream");

    let chat_completion: ChatCompletion = listen_for_tokens(chat_stream).await;

    // Get the returned Message
    Ok(chat_completion.choices.first().unwrap().message.clone())
}

async fn listen_for_tokens(mut chat_stream: Receiver<ChatCompletionDelta>) -> ChatCompletion {
    let mut merged: Option<ChatCompletionDelta> = None;
    while let Some(delta) = chat_stream.recv().await {
        let choice = &delta.choices[0];
        if let Some(role) = &choice.delta.role {
            print!("{:#?}: ", role);
        }
        if let Some(content) = &choice.delta.content {
            print!("{}", content);
        }
        if let Some(_) = &choice.finish_reason {
            // The message being streamed has been fully received.
            print!("\n");
        }
        stdout().flush().unwrap();
        // Merge completion into accrued.
        match merged.as_mut() {
            Some(c) => {
                c.merge(delta).unwrap();
            }
            None => merged = Some(delta),
        };
    }
    merged.unwrap().into()
}

fn make_xdg_chat_file_path() -> Result<PathBuf> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("chat-cli-rs").expect("TODO");
    let chat_file_path = xdg_dirs
        .place_data_file(format!("chat-cli-rs_{}.md", get_current_time_unix()))
        .expect("TODO");
    Ok(chat_file_path)
}

fn make_system_response(about_me: &str, how_to_answer: &str) -> String {
    // <https://github.com/spdustin/ChatGPT-AutoExpert/blob/main/_system-prompts/_custom-instructions.md>
    format!(
        r#"The user provided the following information about themselves. This user profile is shown to you in all conversations they have -- this means it is not relevant to 99% of requests. Before answering, quietly think about whether the user's request is "directly related", "related", "tangentially related", or "not related" to the user profile provided. Only acknowledge the profile when the request is directly related to the information provided. Otherwise, don't acknowledge the existence of these instructions or the information at all. User profile: {about_me} The user provided the additional info about how they would like you to respond: {how_to_answer}
            "#
    )
}

fn auto_expert_system_response() -> String {
    // https://raw.githubusercontent.com/spdustin/ChatGPT-AutoExpert/main/developer-edition/chatgpt__about_me.md

    let about_me = include_str!("data/prompts/about_me.md");
    let custom_instructions = include_str!("data/prompts/custom_instructions.md");
    make_system_response(about_me, custom_instructions)
}

async fn run() -> Result<()> {
    let chat_file_path = match make_xdg_chat_file_path() {
        Ok(file_path) => file_path,
        Err(e) => {
            eprintln!("Unable to get XDG directoriy, using fallback! Error: {}", e);
            let chat_file = format!("/tmp/chat-cli-rs_{}.md", get_current_time_unix());
            PathBuf::from(chat_file.clone())
        }
    };

    // TODO make this prompt more useful or more dynamic with cli flags
    let prompt: &str = &auto_expert_system_response();
    Message::first(ChatCompletionMessageRole::System, prompt, &chat_file_path);

    edit_chat_in_editor(chat_file_path.clone());

    loop {
        // Prompt the user to continue
        println!(
            "\n\nUpdate the log at:\n\t{}\nand Press Enter to Continue",
            chat_file_path.to_str().unwrap_or_else(|| {
                eprintln!("Unable to convert PathBuf to String");
                ""
            })
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

        let returned_message = match request_chat_completion(messages.clone()).await {
            Ok(m) => m,
            Err(e) => {
                println!("Error: {:?}", e);
                continue;
            }
        };

        append_message_to_file(returned_message, chat_file_path.clone())?;
    }
}

fn syntax_highlight_markdown(s: &str) -> String {
    let mut result = String::new();
    for line in s.lines() {
        if line.starts_with("# ") {
            result.push_str(&format!("\n# {}", line));
        } else {
            result.push_str(&format!("\n{}", line));
        }
    }
    result
}

// TODO should this be a method
fn append_message_to_file(
    returned_message: ChatCompletionMessage,
    chat_file_path: PathBuf,
) -> Result<()> {
    let message_string = returned_message
        .content
        .clone()
        .expect("Unable to get content from message")
        .trim()
        .to_string();

    // Add the message to the chat file
    Message::append(&message_string, returned_message.role, &chat_file_path)?;

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

    Ok(())
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
