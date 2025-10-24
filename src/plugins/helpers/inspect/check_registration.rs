use crate::plugins::helpers::database::safe_check_registration;

pub async fn check_registration(user_id: i64) -> bool {
    // Use the centralized safe function, return false on any error
    safe_check_registration(user_id).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_check_registration() {
        let result = check_registration(1234567890).await;
        println!("Registered: {}", result);
    }
}
