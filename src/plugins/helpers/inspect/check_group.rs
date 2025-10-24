use tokio::fs::read_to_string;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub async fn check_group(group_id: i64) -> bool {
    let file_path = "src/resources/auth/groups.txt";
    match read_to_string(file_path).await {
        Ok(contents) => contents.contains(&group_id.to_string()),
        Err(_) => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_check_group() {
        let result = check_group(1234567890).await;
        assert!(result);
    }
}
