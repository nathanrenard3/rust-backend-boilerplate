use super::error::AuthError;
use crate::users::user;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

impl Credentials {
    pub fn normalize(mut self) -> Result<Self, AuthError> {
        self.email = self.email.trim().to_lowercase();
        if self.email.len() > 254 || !email_address::EmailAddress::is_valid(&self.email) {
            return Err(AuthError::InvalidInput("Invalid email address"));
        }
        if self.password.is_empty() || self.password.len() > 1024 {
            return Err(AuthError::InvalidInput(
                "Password must contain 1 to 1024 bytes",
            ));
        }
        Ok(self)
    }
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
}

impl From<user::Model> for UserResponse {
    fn from(user: user::Model) -> Self {
        Self {
            id: user.id,
            email: user.email,
        }
    }
}
