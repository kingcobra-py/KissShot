use teloxide::net::Download;
use teloxide::prelude::Requester;
use teloxide::types::Message;
use teloxide::Bot;

pub async fn read_document_text(bot: &Bot, message: &Message) -> Result<Option<String>, String> {
    let document = match message.document() {
        Some(doc) => doc,
        None => return Ok(None),
    };

    let file_name = document
        .file_name
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();

    let is_text = file_name.ends_with(".txt")
        || file_name.ends_with(".csv")
        || file_name.ends_with(".log")
        || document.mime_type.as_deref() == Some("text/plain");

    if !is_text && !file_name.is_empty() {
        return Err(format!(
            "Unsupported file type: {}. Send a .txt file with cards.",
            document.file_name.as_deref().unwrap_or("unknown")
        ));
    }

    if document.file.size > 20 * 1024 * 1024 {
        return Err("File too large. Telegram bot limit is 20MB.".into());
    }

    let file = bot
        .get_file(document.file.id.clone())
        .await
        .map_err(|e| format!("Failed to get file from Telegram: {}", e))?;

    let temp_path = format!(
        "/tmp/kissshot_{}_{}.txt",
        message.chat.id.0,
        chrono::Utc::now().timestamp_millis()
    );

    let mut dst = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| format!("Failed to create temp file: {}", e))?;

    bot.download_file(&file.path, &mut dst)
        .await
        .map_err(|e| format!("Failed to download file: {}", e))?;

    drop(dst);

    let content = tokio::fs::read_to_string(&temp_path)
        .await
        .map_err(|e| format!("Failed to read downloaded file: {}", e))?;

    let _ = tokio::fs::remove_file(&temp_path).await;

    if content.trim().is_empty() {
        return Err("File is empty.".into());
    }

    Ok(Some(content))
}

pub async fn gather_text_input(
    bot: &Bot,
    message: &Message,
    msg: &str,
    min_len: usize,
) -> Result<String, String> {
    let mut chunks: Vec<String> = Vec::new();

    if msg.trim().len() >= min_len {
        chunks.push(msg.trim().to_string());
    }

    if let Some(text) = read_document_text(bot, message).await? {
        chunks.push(text);
    }

    if let Some(reply) = message.reply_to_message() {
        if let Some(text) = reply.text() {
            if text.trim().len() >= min_len {
                chunks.push(text.trim().to_string());
            }
        }
        if let Some(text) = read_document_text(bot, reply).await? {
            chunks.push(text);
        }
    }

    if chunks.is_empty() {
        return Err(String::new());
    }

    Ok(chunks.join("\n"))
}
