use serde_derive::Deserialize;
use serde_derive::Serialize;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginRequest {
    pub user: UserRequest,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserRequest {
    pub email: String,
    pub password: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityStartRequest {
    pub activity: ActivityStartData,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityStartData {
    pub description: String,
    pub project_id: u64,
    pub started_at: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEndRequest {
    pub activity: ActivityEndData,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEndData {
    pub id: u32,
    pub stopped_at: String,
}
#[derive(Deserialize)]
pub struct ActivityResponse {
    pub id: u32,
}

impl ActivityStartRequest {
    pub fn new(description: String, project_id: u64, started_at: String) -> Self {
        ActivityStartRequest {
            activity: ActivityStartData {
                description,
                project_id,
                started_at,
            },
        }
    }
}

impl ActivityEndRequest {
    pub fn new(id: u32, stopped_at: String) -> Self {
        ActivityEndRequest {
            activity: ActivityEndData { id, stopped_at },
        }
    }
}
