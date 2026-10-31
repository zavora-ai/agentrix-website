//! The seven charter roles: the names owner-facing surfaces use for jobs.
//!
//! | Job | Role |
//! |---|---|
//! | `protect` | Security Guard |
//! | `hosting` | Facilities Manager |
//! | `freshness` | Content Editor |
//! | `social` | Reputation Manager |
//! | `nurture` | Sales Rep |
//! | `owner-questions` | Chief of Staff |
//! | `weekly-review` | Analyst |

/// The role name for `job`, or `None` when the job is not one of the seven:
/// unknown jobs never get a made-up name on a card.
pub fn role_of(job: &str) -> Option<&'static str> {
    match job {
        "protect" => Some("Security Guard"),
        "hosting" => Some("Facilities Manager"),
        "freshness" => Some("Content Editor"),
        "social" => Some("Reputation Manager"),
        "nurture" => Some("Sales Rep"),
        "owner-questions" => Some("Chief of Staff"),
        "weekly-review" => Some("Analyst"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_seven_jobs_have_roles_and_nothing_else_does() {
        let jobs = [
            ("protect", "Security Guard"),
            ("hosting", "Facilities Manager"),
            ("freshness", "Content Editor"),
            ("social", "Reputation Manager"),
            ("nurture", "Sales Rep"),
            ("owner-questions", "Chief of Staff"),
            ("weekly-review", "Analyst"),
        ];
        for (job, role) in jobs {
            assert_eq!(role_of(job), Some(role));
        }
        assert_eq!(role_of("freshnes"), None);
        assert_eq!(role_of(""), None);
        assert_eq!(role_of("onboarding"), None);
    }
}
