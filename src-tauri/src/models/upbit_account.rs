use serde::{Deserialize, Serialize};

/// Upbit API 계정 1개. 키 값은 별도 keyring에 `upbit_access_key_<id>` /
/// `upbit_secret_key_<id>`로 저장되며 이 구조체에는 포함되지 않는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpbitAccount {
    pub id: i64,
    pub user_id: i64,
    pub label: String,
    pub enabled: bool,
    pub created_at: String,
    /// 키링에 access 키가 저장되어 있는지 (값 노출 X).
    pub has_access_key: bool,
    /// 키링에 secret 키가 저장되어 있는지.
    pub has_secret_key: bool,
    /// 이 계정에 running 세션이 있는지 (UI 셀렉터에서 disabled 표시용).
    /// JOIN으로 계산되어 list 응답에 포함됨.
    pub has_running_session: bool,
    /// 계정 전용 Discord webhook URL. NULL이면 `notification_configs`의
    /// 글로벌 webhook을 fallback으로 사용한다. 다중 사용자가 시청하는
    /// 환경에서 계정별로 다른 채널에 알림을 보내고 싶을 때 사용.
    #[serde(default)]
    pub discord_webhook_url: Option<String>,
}
