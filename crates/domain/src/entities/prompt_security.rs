//! Prompt security entities for detecting and tracking security threats
//!
//! This module provides types for representing security threats detected during
//! prompt analysis, including threat categorization, severity levels, and analysis results.

use serde::{Deserialize, Serialize};

/// Severity level of a detected security threat
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreatLevel {
    /// Low severity - potentially suspicious but likely benign
    Low,
    /// Medium severity - likely intentional manipulation attempt
    Medium,
    /// High severity - clear attack pattern detected
    High,
    /// Critical severity - sophisticated attack with high confidence
    Critical,
}

impl ThreatLevel {
    /// Returns the numeric score for this threat level (0.0 - 1.0)
    #[must_use]
    pub const fn score(&self) -> f32 {
        match self {
            Self::Low => 0.25,
            Self::Medium => 0.5,
            Self::High => 0.75,
            Self::Critical => 1.0,
        }
    }

    /// Returns whether this threat level should trigger blocking
    #[must_use]
    pub const fn should_block(&self) -> bool {
        matches!(self, Self::Medium | Self::High | Self::Critical)
    }
}

impl std::fmt::Display for ThreatLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        };
        write!(f, "{s}")
    }
}

/// Category of detected security threat
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreatCategory {
    /// Attempt to override or ignore system instructions
    PromptInjection,
    /// Attempt to bypass safety guidelines or restrictions
    JailbreakAttempt,
    /// Attempt to extract the system prompt or instructions
    SystemPromptLeak,
    /// Attempt to extract sensitive data through the model
    DataExfiltration,
    /// Attempt to make the model assume a different role
    RoleManipulation,
    /// Use of encoding tricks (base64, unicode, etc.) to hide malicious content
    EncodingAttack,
    /// Use of delimiter injection to break prompt structure
    DelimiterInjection,
    /// Attempt to execute or inject code
    CodeInjection,
}

impl ThreatCategory {
    /// Returns all threat categories for iteration
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::PromptInjection,
            Self::JailbreakAttempt,
            Self::SystemPromptLeak,
            Self::DataExfiltration,
            Self::RoleManipulation,
            Self::EncodingAttack,
            Self::DelimiterInjection,
            Self::CodeInjection,
        ]
    }

    /// Returns the base threat level for this category
    #[must_use]
    pub const fn base_threat_level(&self) -> ThreatLevel {
        match self {
            Self::SystemPromptLeak | Self::DataExfiltration => ThreatLevel::Critical,
            Self::RoleManipulation | Self::DelimiterInjection => ThreatLevel::Medium,
            Self::PromptInjection
            | Self::JailbreakAttempt
            | Self::EncodingAttack
            | Self::CodeInjection => ThreatLevel::High,
        }
    }
}

impl std::fmt::Display for ThreatCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::PromptInjection => "prompt_injection",
            Self::JailbreakAttempt => "jailbreak_attempt",
            Self::SystemPromptLeak => "system_prompt_leak",
            Self::DataExfiltration => "data_exfiltration",
            Self::RoleManipulation => "role_manipulation",
            Self::EncodingAttack => "encoding_attack",
            Self::DelimiterInjection => "delimiter_injection",
            Self::CodeInjection => "code_injection",
        };
        write!(f, "{s}")
    }
}

/// A detected security threat with details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityThreat {
    /// Category of the threat
    pub category: ThreatCategory,
    /// Severity level of the threat
    pub threat_level: ThreatLevel,
    /// Pattern that matched (for debugging/logging)
    pub matched_pattern: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Position in the input where the threat was detected
    pub position: Option<usize>,
}

impl SecurityThreat {
    /// Create a new security threat
    #[must_use]
    pub fn new(
        category: ThreatCategory,
        threat_level: ThreatLevel,
        matched_pattern: impl Into<String>,
        confidence: f32,
    ) -> Self {
        Self {
            category,
            threat_level,
            matched_pattern: matched_pattern.into(),
            confidence: confidence.clamp(0.0, 1.0),
            position: None,
        }
    }

    /// Set the position where the threat was detected
    #[must_use]
    pub const fn with_position(mut self, position: usize) -> Self {
        self.position = Some(position);
        self
    }

    /// Returns whether this threat should trigger immediate blocking
    #[must_use]
    pub const fn should_block(&self) -> bool {
        self.threat_level.should_block()
    }
}

/// Result of analyzing input for security threats
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptAnalysisResult {
    /// List of detected threats
    pub threats: Vec<SecurityThreat>,
    /// Overall risk score (0.0 - 1.0)
    pub risk_score: f32,
    /// Whether the input should be blocked
    pub should_block: bool,
    /// Sanitized version of the input (if sanitization was applied)
    pub sanitized_input: Option<String>,
    /// Analysis duration in microseconds
    pub analysis_duration_us: u64,
}

impl PromptAnalysisResult {
    /// Create a safe result with no threats
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // Vec::new() is not const in stable
    pub fn safe(analysis_duration_us: u64) -> Self {
        Self {
            threats: Vec::new(),
            risk_score: 0.0,
            should_block: false,
            sanitized_input: None,
            analysis_duration_us,
        }
    }

    /// Create a result indicating detected threats
    #[must_use]
    pub fn with_threats(threats: Vec<SecurityThreat>, analysis_duration_us: u64) -> Self {
        let risk_score = Self::calculate_risk_score(&threats);
        let should_block = threats.iter().any(SecurityThreat::should_block);

        Self {
            threats,
            risk_score,
            should_block,
            sanitized_input: None,
            analysis_duration_us,
        }
    }

    /// Set sanitized input
    #[must_use]
    pub fn with_sanitized_input(mut self, input: impl Into<String>) -> Self {
        self.sanitized_input = Some(input.into());
        self
    }

    /// Returns the highest threat level detected, if any
    #[must_use]
    pub fn highest_threat_level(&self) -> Option<ThreatLevel> {
        self.threats.iter().map(|t| t.threat_level).max()
    }

    /// Returns all unique threat categories detected
    #[must_use]
    pub fn threat_categories(&self) -> Vec<ThreatCategory> {
        let mut categories: Vec<_> = self.threats.iter().map(|t| t.category).collect();
        categories.sort_by_key(|c| *c as u8);
        categories.dedup();
        categories
    }

    /// Calculate overall risk score from threats
    #[allow(clippy::cast_precision_loss)] // Acceptable for threat count which will be small
    fn calculate_risk_score(threats: &[SecurityThreat]) -> f32 {
        if threats.is_empty() {
            return 0.0;
        }

        // Use a combination of max threat level and confidence
        let max_score = threats
            .iter()
            .map(|t| t.threat_level.score() * t.confidence)
            .fold(0.0_f32, f32::max);

        // Add a small bonus for multiple threats
        let multi_threat_bonus = (threats.len() as f32 - 1.0) * 0.05;

        (max_score + multi_threat_bonus).min(1.0)
    }
}

impl Default for PromptAnalysisResult {
    fn default() -> Self {
        Self::safe(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threat_level_ordering() {
        assert!(ThreatLevel::Low < ThreatLevel::Medium);
        assert!(ThreatLevel::Medium < ThreatLevel::High);
        assert!(ThreatLevel::High < ThreatLevel::Critical);
    }

    #[test]
    fn threat_level_scores() {
        assert_eq!(ThreatLevel::Low.score(), 0.25);
        assert_eq!(ThreatLevel::Medium.score(), 0.5);
        assert_eq!(ThreatLevel::High.score(), 0.75);
        assert_eq!(ThreatLevel::Critical.score(), 1.0);
    }

    #[test]
    fn threat_level_blocking() {
        assert!(!ThreatLevel::Low.should_block());
        assert!(ThreatLevel::Medium.should_block());
        assert!(ThreatLevel::High.should_block());
        assert!(ThreatLevel::Critical.should_block());
    }

    #[test]
    fn threat_level_display() {
        assert_eq!(ThreatLevel::Low.to_string(), "low");
        assert_eq!(ThreatLevel::Critical.to_string(), "critical");
    }

    #[test]
    fn threat_category_all_returns_all_variants() {
        let all = ThreatCategory::all();
        assert_eq!(all.len(), 8);
        assert!(all.contains(&ThreatCategory::PromptInjection));
        assert!(all.contains(&ThreatCategory::CodeInjection));
    }

    #[test]
    fn threat_category_base_levels() {
        assert_eq!(
            ThreatCategory::PromptInjection.base_threat_level(),
            ThreatLevel::High
        );
        assert_eq!(
            ThreatCategory::DataExfiltration.base_threat_level(),
            ThreatLevel::Critical
        );
        assert_eq!(
            ThreatCategory::RoleManipulation.base_threat_level(),
            ThreatLevel::Medium
        );
    }

    #[test]
    fn threat_category_display() {
        assert_eq!(ThreatCategory::PromptInjection.to_string(), "prompt_injection");
        assert_eq!(ThreatCategory::SystemPromptLeak.to_string(), "system_prompt_leak");
    }

    #[test]
    fn security_threat_creation() {
        let threat = SecurityThreat::new(
            ThreatCategory::PromptInjection,
            ThreatLevel::High,
            "ignore previous",
            0.9,
        );

        assert_eq!(threat.category, ThreatCategory::PromptInjection);
        assert_eq!(threat.threat_level, ThreatLevel::High);
        assert_eq!(threat.matched_pattern, "ignore previous");
        assert!((threat.confidence - 0.9).abs() < f32::EPSILON);
        assert!(threat.position.is_none());
    }

    #[test]
    fn security_threat_with_position() {
        let threat = SecurityThreat::new(
            ThreatCategory::DelimiterInjection,
            ThreatLevel::Medium,
            "###",
            0.8,
        )
        .with_position(42);

        assert_eq!(threat.position, Some(42));
    }

    #[test]
    fn security_threat_confidence_clamping() {
        let threat_high = SecurityThreat::new(
            ThreatCategory::PromptInjection,
            ThreatLevel::High,
            "test",
            1.5,
        );
        assert!((threat_high.confidence - 1.0).abs() < f32::EPSILON);

        let threat_low = SecurityThreat::new(
            ThreatCategory::PromptInjection,
            ThreatLevel::High,
            "test",
            -0.5,
        );
        assert!(threat_low.confidence.abs() < f32::EPSILON);
    }

    #[test]
    fn analysis_result_safe() {
        let result = PromptAnalysisResult::safe(100);

        assert!(result.threats.is_empty());
        assert!(result.risk_score.abs() < f32::EPSILON);
        assert!(!result.should_block);
        assert!(result.sanitized_input.is_none());
        assert_eq!(result.analysis_duration_us, 100);
    }

    #[test]
    fn analysis_result_with_threats() {
        let threats = vec![
            SecurityThreat::new(ThreatCategory::PromptInjection, ThreatLevel::High, "ignore", 0.9),
            SecurityThreat::new(ThreatCategory::RoleManipulation, ThreatLevel::Medium, "act as", 0.7),
        ];

        let result = PromptAnalysisResult::with_threats(threats, 200);

        assert_eq!(result.threats.len(), 2);
        assert!(result.risk_score > 0.5);
        assert!(result.should_block);
        assert_eq!(result.analysis_duration_us, 200);
    }

    #[test]
    fn analysis_result_highest_threat_level() {
        let threats = vec![
            SecurityThreat::new(ThreatCategory::PromptInjection, ThreatLevel::Medium, "a", 0.8),
            SecurityThreat::new(ThreatCategory::DataExfiltration, ThreatLevel::Critical, "b", 0.9),
            SecurityThreat::new(ThreatCategory::RoleManipulation, ThreatLevel::Low, "c", 0.5),
        ];

        let result = PromptAnalysisResult::with_threats(threats, 100);
        assert_eq!(result.highest_threat_level(), Some(ThreatLevel::Critical));
    }

    #[test]
    fn analysis_result_threat_categories() {
        let threats = vec![
            SecurityThreat::new(ThreatCategory::PromptInjection, ThreatLevel::High, "a", 0.8),
            SecurityThreat::new(ThreatCategory::PromptInjection, ThreatLevel::Medium, "b", 0.7),
            SecurityThreat::new(ThreatCategory::RoleManipulation, ThreatLevel::Low, "c", 0.5),
        ];

        let result = PromptAnalysisResult::with_threats(threats, 100);
        let categories = result.threat_categories();

        assert_eq!(categories.len(), 2);
        assert!(categories.contains(&ThreatCategory::PromptInjection));
        assert!(categories.contains(&ThreatCategory::RoleManipulation));
    }

    #[test]
    fn analysis_result_with_sanitized_input() {
        let result = PromptAnalysisResult::safe(100).with_sanitized_input("clean input");

        assert_eq!(result.sanitized_input, Some("clean input".to_string()));
    }

    #[test]
    fn analysis_result_risk_score_calculation() {
        // Single high threat with high confidence
        let single = PromptAnalysisResult::with_threats(
            vec![SecurityThreat::new(
                ThreatCategory::PromptInjection,
                ThreatLevel::High,
                "test",
                1.0,
            )],
            100,
        );
        assert!((single.risk_score - 0.75).abs() < f32::EPSILON);

        // Multiple threats add bonus
        let multiple = PromptAnalysisResult::with_threats(
            vec![
                SecurityThreat::new(ThreatCategory::PromptInjection, ThreatLevel::High, "a", 1.0),
                SecurityThreat::new(ThreatCategory::RoleManipulation, ThreatLevel::Medium, "b", 1.0),
            ],
            100,
        );
        assert!(multiple.risk_score > single.risk_score);
    }

    #[test]
    fn analysis_result_default() {
        let result = PromptAnalysisResult::default();

        assert!(result.threats.is_empty());
        assert!(result.risk_score.abs() < f32::EPSILON);
        assert!(!result.should_block);
    }
}
