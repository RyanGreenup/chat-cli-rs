# Chat GPT as a Markdown Buffer with Rust

This is a simple program to interacti with OpenAI's GPT models. It breakes down a markdown file into user/assistant messages and uses the `openai` crate to get responses. This is convenient because it archives the chat as a markdown file.

## Features
- Send and receive messages as a markdown file
  - Cache's Conversation
  - Allows rewriting conversation
- Never leave Vim / Emacs / VSCode
- Sends desktop notifications
- Uses XDG location

### TODO Features
- [ ] Implement Async Streaming
- [ ] Implement different prompts (e.g. e.g. [^1] [^2]) with a fzf selector
- [ ] Implement different models with a fzf selector
- [ ] Interact with `ollama` or the OObabooga Server

[^1]: https://raw.githubusercontent.com/spdustin/ChatGPT-AutoExpert/main/developer-edition/chatgpt__custom_instructions.md
[^2]: https://spdustin.substack.com/p/autoexpert-custom-instructions-for

## How to Use

1. Export `OPENAI_API_KEY`
2. Clone: `git clone https://github.com/RyanGreenup/chat-cli-rs`
3. Compile: `cd chat-cli-rs && cargo install --path .`
4. Recommended workflow
  - Start vim
  - Open the integrated terminal
  - `chat-cli-rs`
  - Open the buffer in vim and edit
  - Press Enter on the Terminal to send the chat up to OpenAI for Completion

## Dependencies

This CLI only depends on rust crates but it does require `libssl.so.1.1`.
