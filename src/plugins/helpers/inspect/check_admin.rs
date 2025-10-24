use crate::database::*;
use crate::handle_database_error;
use crate::safe_database_operation;
pub async fn check_admin(user_id: i64) -> bool {
    let sql = safe_database_operation!(get_sql().await, return false);
    let target_user = safe_database_operation!(fetch_user(user_id).await, return false);
    if target_user.is_none() {
        return false;
    }
    let target_user = target_user.unwrap();
    if target_user.status == "ADMIN" {
        return true;
    }
    return false;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_check_admin() {
        let result = check_admin(766109755).await;
        dbg!(result);
        assert!(result);
    }
}
