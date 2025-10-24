use crate::database::*;
use crate::handle_database_error;
use crate::safe_database_operation;
pub async fn check_banned(user_id: i64) -> bool {
    let sql = safe_database_operation!(get_sql().await, return false);
    let user = safe_database_operation!(fetch_user(user_id).await, return false);
    if user.is_none() {
        return false;
    }
    let user = user.unwrap();
    if user.status == "BANNED" {
        return true;
    }
    return false;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_check_banned() {
        let result = check_banned(1234567890).await;
        assert!(result);
    }
}
