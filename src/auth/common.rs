use serde::Serialize;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::AuthConfig;

/// Information about a token source for comparison and decision-making
#[derive(Debug, Clone, Serialize)]
pub struct TokenInfo {
    pub source: String,
    pub available: bool,
    pub expires_at: Option<u64>,
    pub is_expired: bool,
    pub age_description: String,
}

impl TokenInfo {
    /// Create an unavailable token info for the given source
    pub fn unavailable(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            available: false,
            expires_at: None,
            is_expired: true,
            age_description: "unavailable".to_string(),
        }
    }
}

impl fmt::Display for TokenInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.available {
            write!(f, "{} - unavailable", self.source)
        } else if self.is_expired {
            write!(f, "{} - EXPIRED ({})", self.source, self.age_description)
        } else {
            write!(f, "{} - valid ({})", self.source, self.age_description)
        }
    }
}

/// Decision about which token source to use with rationale
#[derive(Debug, Clone, Serialize)]
pub struct TokenDecision {
    pub source: String,
    pub rationale: String,
}

impl fmt::Display for TokenDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.source, self.rationale)
    }
}

/// Analyze a token source and return detailed information
pub fn analyze_token_source(source_name: &str, auth_config: &AuthConfig) -> TokenInfo {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let has_tokens = auth_config.oauth_access_token.is_some();
    
    if !has_tokens {
        return TokenInfo {
            source: source_name.to_string(),
            available: false,
            expires_at: None,
            is_expired: true,
            age_description: "no tokens".to_string(),
        };
    }

    let expires_at = auth_config.oauth_expires.unwrap_or(0);
    let is_expired = expires_at <= now;
    
    let age_description = if expires_at == 0 {
        "unknown expiry".to_string()
    } else if is_expired {
        let expired_ago = (now - expires_at) / 1000 / 60; // minutes ago
        if expired_ago < 60 {
            format!("expired {}m ago", expired_ago)
        } else {
            format!("expired {}h ago", expired_ago / 60)
        }
    } else {
        let expires_in = (expires_at - now) / 1000 / 60; // minutes from now
        if expires_in < 60 {
            format!("expires in {}m", expires_in)
        } else if expires_in < 1440 {
            format!("expires in {}h", expires_in / 60)
        } else {
            format!("expires in {}d", expires_in / 1440)
        }
    };

    TokenInfo {
        source: source_name.to_string(),
        available: true,
        expires_at: Some(expires_at),
        is_expired,
        age_description,
    }
}

/// Check if OAuth tokens exist anywhere (even if expired) - indicates user has subscription
pub fn oauth_tokens_exist_anywhere() -> bool {
    // Check Claude CLI tokens
    let claude_exists = crate::auth::anthropic::AnthropicOAuth::try_claude_code_credentials().is_ok();
    
    // Check Gemini CLI tokens  
    let gemini_exists = crate::auth::google::GoogleOAuth::try_gemini_cli_credentials().is_ok();
    
    claude_exists || gemini_exists
}

/// Choose the best token source based on availability and freshness
pub fn choose_best_token_source(primary_info: &TokenInfo, secondary_info: &TokenInfo) -> TokenDecision {
    // Neither available
    if !primary_info.available && !secondary_info.available {
        return TokenDecision {
            source: "none".to_string(),
            rationale: "No tokens available from either source".to_string(),
        };
    }

    // Only one available
    if primary_info.available && !secondary_info.available {
        return TokenDecision {
            source: primary_info.source.clone(),
            rationale: format!("Only {} tokens available", primary_info.source),
        };
    }
    if !primary_info.available && secondary_info.available {
        return TokenDecision {
            source: secondary_info.source.clone(),
            rationale: format!("Only {} tokens available", secondary_info.source),
        };
    }

    // Both available - compare freshness
    let primary_expires = primary_info.expires_at.unwrap_or(0);
    let secondary_expires = secondary_info.expires_at.unwrap_or(0);

    // Both expired - choose the less expired one
    if primary_info.is_expired && secondary_info.is_expired {
        if primary_expires > secondary_expires {
            return TokenDecision {
                source: primary_info.source.clone(),
                rationale: format!("Both expired, {} tokens less stale ({} vs {})", 
                                   primary_info.source, primary_info.age_description, secondary_info.age_description),
            };
        } else {
            return TokenDecision {
                source: secondary_info.source.clone(),
                rationale: format!("Both expired, {} tokens less stale ({} vs {})", 
                                   secondary_info.source, secondary_info.age_description, primary_info.age_description),
            };
        }
    }

    // One expired, one valid - choose the valid one
    if primary_info.is_expired && !secondary_info.is_expired {
        return TokenDecision {
            source: secondary_info.source.clone(),
            rationale: format!("{} valid ({}), {} expired ({})", 
                               secondary_info.source, secondary_info.age_description, 
                               primary_info.source, primary_info.age_description),
        };
    }
    if !primary_info.is_expired && secondary_info.is_expired {
        return TokenDecision {
            source: primary_info.source.clone(),
            rationale: format!("{} valid ({}), {} expired ({})", 
                               primary_info.source, primary_info.age_description, 
                               secondary_info.source, secondary_info.age_description),
        };
    }

    // Both valid - choose the one that expires later (fresher)
    if secondary_expires > primary_expires {
        TokenDecision {
            source: secondary_info.source.clone(),
            rationale: format!("{} newer ({} vs {})", 
                               secondary_info.source, secondary_info.age_description, primary_info.age_description),
        }
    } else {
        TokenDecision {
            source: primary_info.source.clone(),
            rationale: format!("{} newer ({} vs {})", 
                               primary_info.source, primary_info.age_description, secondary_info.age_description),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_auth_config(access_token: bool, expires: Option<u64>) -> AuthConfig {
        AuthConfig {
            oauth_access_token: if access_token { Some("token".to_string()) } else { None },
            oauth_refresh_token: None,
            oauth_expires: expires,
            project_id: None,
        }
    }

    #[test]
    fn test_analyze_token_source_no_tokens() {
        let config = create_auth_config(false, None);
        let info = analyze_token_source("test", &config);
        
        assert_eq!(info.source, "test");
        assert!(!info.available);
        assert!(info.is_expired);
        assert_eq!(info.age_description, "no tokens");
    }

    #[test]
    fn test_analyze_token_source_valid_token() {
        let future_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64 + 3600000; // 1 hour from now

        let config = create_auth_config(true, Some(future_time));
        let info = analyze_token_source("test", &config);
        
        assert_eq!(info.source, "test");
        assert!(info.available);
        assert!(!info.is_expired);
        assert!(info.age_description.contains("expires in"));
    }

    #[test]
    fn test_choose_best_token_both_unavailable() {
        let primary = TokenInfo {
            source: "primary".to_string(),
            available: false,
            expires_at: None,
            is_expired: true,
            age_description: "no tokens".to_string(),
        };
        let secondary = primary.clone();
        
        let decision = choose_best_token_source(&primary, &secondary);
        assert_eq!(decision.source, "none");
    }

    #[test]
    fn test_choose_best_token_one_available() {
        let primary = TokenInfo {
            source: "primary".to_string(),
            available: true,
            expires_at: Some(123456),
            is_expired: false,
            age_description: "valid".to_string(),
        };
        let secondary = TokenInfo {
            source: "secondary".to_string(),
            available: false,
            expires_at: None,
            is_expired: true,
            age_description: "no tokens".to_string(),
        };
        
        let decision = choose_best_token_source(&primary, &secondary);
        assert_eq!(decision.source, "primary");
    }

    #[test]
    fn test_choose_best_token_both_valid_newer_wins() {
        let primary = TokenInfo {
            source: "primary".to_string(),
            available: true,
            expires_at: Some(100000),
            is_expired: false,
            age_description: "expires in 30m".to_string(),
        };
        let secondary = TokenInfo {
            source: "secondary".to_string(),
            available: true,
            expires_at: Some(200000), // Expires later (newer)
            is_expired: false,
            age_description: "expires in 60m".to_string(),
        };
        
        let decision = choose_best_token_source(&primary, &secondary);
        assert_eq!(decision.source, "secondary");
        assert!(decision.rationale.contains("newer"));
    }
}