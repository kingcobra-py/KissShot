use crate::database::*;
use crate::handle_database_error;
use crate::safe_database_operation;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
pub async fn check_access(chat_id: i64) -> bool {
    let file_path = "src/resources/auth/groups.txt";
    if let Ok(mut file) = File::open(file_path).await {
        let mut contents = String::new();
        if file.read_to_string(&mut contents).await.is_ok() {
            if contents.contains(&chat_id.to_string()) {
                return true;
            }
        }
    }

    let sql = safe_database_operation!(get_sql().await, return false);
    let user = safe_database_operation!(fetch_user(chat_id).await, return false);

    if let Some(u) = user {
        if u.status == "ADMIN" || u.status == "PREMIUM" {
            return true;
        }
    }

    false
}
