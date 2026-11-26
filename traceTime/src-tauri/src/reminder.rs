use serde::{Deserialize, Serialize};

const DEFAULT_EST_SECONDS: i64 = 60;
const DEFAULT_OVERDUE_INTERVAL_MINUTES: i64 = 10;
pub const OVERDUE_INTERVAL_KEY: &str = "reminder_overdue_interval_minutes";

fn valid_integer(value: &str, range: std::ops::RangeInclusive<i64>) -> Option<i64> {
    value
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|number| range.contains(number))
}

pub fn parse_overdue_interval(value: Option<&str>) -> Result<i64, String> {
    match value {
        None => Ok(DEFAULT_OVERDUE_INTERVAL_MINUTES),
        Some(value) => valid_integer(value, 1..=1440)
            .ok_or_else(|| format!("invalid integer setting {OVERDUE_INTERVAL_KEY}: {value}")),
    }
}

pub fn load_overdue_interval(value: Option<&str>) -> i64 {
    parse_overdue_interval(value).unwrap_or(DEFAULT_OVERDUE_INTERVAL_MINUTES)
}

#[derive(Debug, Clone)]
pub struct ReminderConfig {
    pub reminder_enabled: bool,
    pub warn_enabled: bool,
    pub warn_percent: i64,
    pub near_enabled: bool,
    pub near_percent: i64,
    pub due_enabled: bool,
    pub due_percent: i64,
    pub overdue_enabled: bool,
    pub overdue_notify: bool,
    pub overdue_interval_minutes: i64,
    pub warn_sound: bool,
    pub near_sound: bool,
    pub due_notify: bool,
    est_seconds: i64,
    overdue_interval_from_net: Option<i64>,
    overdue_upgrade_cutoff_net: Option<i64>,
}

impl Default for ReminderConfig {
    fn default() -> Self {
        Self {
            reminder_enabled: true,
            warn_enabled: true,
            warn_percent: 75,
            near_enabled: true,
            near_percent: 90,
            due_enabled: true,
            due_percent: 100,
            overdue_enabled: true,
            overdue_notify: true,
            overdue_interval_minutes: DEFAULT_OVERDUE_INTERVAL_MINUTES,
            warn_sound: false,
            near_sound: false,
            due_notify: true,
            est_seconds: DEFAULT_EST_SECONDS,
            overdue_interval_from_net: None,
            overdue_upgrade_cutoff_net: None,
        }
    }
}

impl ReminderConfig {
    pub fn from_settings(
        get: impl Fn(&str) -> Result<Option<String>, String>,
    ) -> Result<Self, String> {
        fn bool_value(
            get: &impl Fn(&str) -> Result<Option<String>, String>,
            key: &str,
            default: bool,
        ) -> Result<bool, String> {
            Ok(match get(key)?.as_deref().map(str::trim) {
                Some("true" | "1") => true,
                Some("false" | "0") => false,
                _ => default,
            })
        }

        fn int_value(
            get: &impl Fn(&str) -> Result<Option<String>, String>,
            key: &str,
            default: i64,
            range: std::ops::RangeInclusive<i64>,
        ) -> Result<i64, String> {
            Ok(get(key)?
                .and_then(|value| valid_integer(&value, range))
                .unwrap_or(default))
        }

        let defaults = Self::default();
        let mut percentages = [
            int_value(
                &get,
                "reminder_warn_percent",
                defaults.warn_percent,
                1..=1000,
            )?,
            int_value(
                &get,
                "reminder_near_percent",
                defaults.near_percent,
                1..=1000,
            )?,
            int_value(&get, "reminder_due_percent", defaults.due_percent, 1..=1000)?,
        ];
        percentages.sort_unstable();

        Ok(Self {
            reminder_enabled: bool_value(&get, "reminder_enabled", defaults.reminder_enabled)?,
            warn_enabled: bool_value(&get, "reminder_warn_enabled", defaults.warn_enabled)?,
            warn_percent: percentages[0],
            near_enabled: bool_value(&get, "reminder_near_enabled", defaults.near_enabled)?,
            near_percent: percentages[1],
            due_enabled: bool_value(&get, "reminder_due_enabled", defaults.due_enabled)?,
            due_percent: percentages[2],
            overdue_enabled: bool_value(
                &get,
                "reminder_overdue_enabled",
                defaults.overdue_enabled,
            )?,
            overdue_notify: bool_value(&get, "reminder_overdue_notify", defaults.overdue_notify)?,
            overdue_interval_minutes: load_overdue_interval(get(OVERDUE_INTERVAL_KEY)?.as_deref()),
            warn_sound: bool_value(&get, "reminder_warn_sound", defaults.warn_sound)?,
            near_sound: bool_value(&get, "reminder_near_sound", defaults.near_sound)?,
            due_notify: bool_value(&get, "reminder_due_notify", defaults.due_notify)?,
            ..defaults
        })
    }

    pub(crate) fn for_task(
        mut self,
        est_seconds: i64,
        overdue_interval_from_net: Option<i64>,
        overdue_upgrade_cutoff_net: Option<i64>,
    ) -> Self {
        self.est_seconds = est_seconds;
        self.overdue_interval_from_net = overdue_interval_from_net;
        self.overdue_upgrade_cutoff_net = overdue_upgrade_cutoff_net;
        self
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReminderConfigDto {
    pub reminder_enabled: bool,
    pub warn_enabled: bool,
    pub warn_percent: i64,
    pub near_enabled: bool,
    pub near_percent: i64,
    pub due_enabled: bool,
    pub due_percent: i64,
    pub overdue_enabled: bool,
    pub overdue_notify: bool,
    pub overdue_interval_minutes: i64,
    pub warn_sound: bool,
    pub near_sound: bool,
    pub due_notify: bool,
}

impl From<&ReminderConfig> for ReminderConfigDto {
    fn from(config: &ReminderConfig) -> Self {
        Self {
            reminder_enabled: config.reminder_enabled,
            warn_enabled: config.warn_enabled,
            warn_percent: config.warn_percent,
            near_enabled: config.near_enabled,
            near_percent: config.near_percent,
            due_enabled: config.due_enabled,
            due_percent: config.due_percent,
            overdue_enabled: config.overdue_enabled,
            overdue_notify: config.overdue_notify,
            overdue_interval_minutes: config.overdue_interval_minutes,
            warn_sound: config.warn_sound,
            near_sound: config.near_sound,
            due_notify: config.due_notify,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReminderLevel {
    Warn,
    Near,
    Due,
    Overdue,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct DueReminder {
    pub node: String,
    pub level: ReminderLevel,
    pub title: String,
    pub body: String,
    pub notify: bool,
    pub sound: bool,
}

impl DueReminder {
    pub fn notification_allowed(&self, task_muted: bool) -> bool {
        self.notify
            && match self.level {
                ReminderLevel::Due => true,
                ReminderLevel::Overdue => !task_muted,
                ReminderLevel::Warn | ReminderLevel::Near => false,
            }
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReminderCheckDto {
    pub net_seconds: i64,
    pub config: ReminderConfigDto,
    pub reminders: Vec<DueReminder>,
}

fn threshold_seconds(est_seconds: i64, percent: i64) -> i64 {
    ((est_seconds as i128 * percent as i128) / 100).clamp(0, i64::MAX as i128) as i64
}

pub fn due_reminders(net_seconds: i64, config: &ReminderConfig) -> Vec<DueReminder> {
    if !config.reminder_enabled {
        return Vec::new();
    }

    let net_seconds = net_seconds.max(0);
    let due_at = threshold_seconds(config.est_seconds, config.due_percent);
    let mut reminders = Vec::new();
    if config.warn_enabled
        && net_seconds >= threshold_seconds(config.est_seconds, config.warn_percent)
    {
        reminders.push(DueReminder {
            node: "warn".into(),
            level: ReminderLevel::Warn,
            title: "接近预计时长".into(),
            body: format!("已使用预计时长的 {}%", config.warn_percent),
            notify: false,
            sound: config.warn_sound,
        });
    }
    if config.near_enabled
        && net_seconds >= threshold_seconds(config.est_seconds, config.near_percent)
    {
        reminders.push(DueReminder {
            node: "near".into(),
            level: ReminderLevel::Near,
            title: "即将达到预计时长".into(),
            body: format!("已使用预计时长的 {}%", config.near_percent),
            notify: false,
            sound: config.near_sound,
        });
    }
    if config.due_enabled && net_seconds >= due_at {
        reminders.push(DueReminder {
            node: "due".into(),
            level: ReminderLevel::Due,
            title: "已达到预计时长".into(),
            body: "请尽快收尾".into(),
            notify: config.due_notify,
            sound: false,
        });
    }
    // Overdue tasks at upgrade start a fresh interval at the saved net time.
    // Otherwise the due deadline is the anchor. A late poll considers only the
    // newest reached milestone, and interval edits never backfill past nodes.
    let interval = i128::from(config.overdue_interval_minutes) * 60;
    let due_at = i128::from(due_at);
    let anchor = i128::from(config.overdue_upgrade_cutoff_net.unwrap_or(0)).max(due_at);
    let net = i128::from(net_seconds);
    let repeats = (net - anchor).max(0) / interval;
    let reached = anchor + repeats * interval;
    if config.overdue_enabled
        && repeats > 0
        && config
            .overdue_interval_from_net
            .is_none_or(|from| reached >= i128::from(from))
    {
        let reached_milestone = reached - due_at;
        let current_overtime = net - due_at;
        reminders.push(DueReminder {
            node: format!("overdue:at:{reached_milestone}"),
            level: ReminderLevel::Overdue,
            title: "任务已超时".into(),
            body: format!("已超过到期时间 {} 分钟", current_overtime / 60),
            notify: config.overdue_notify,
            sound: false,
        });
    }
    reminders
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_maps_all_four_stages() {
        let config = ReminderConfig::default();
        let due = due_reminders(46, &config); // 75% of 60s
        assert_eq!(
            due.iter().map(|x| x.node.as_str()).collect::<Vec<_>>(),
            vec!["warn"]
        );
        let due = due_reminders(55, &config);
        assert_eq!(
            due.iter().map(|x| x.node.as_str()).collect::<Vec<_>>(),
            vec!["warn", "near"]
        );
        let due = due_reminders(60, &config);
        assert_eq!(
            due.iter().map(|x| x.node.as_str()).collect::<Vec<_>>(),
            vec!["warn", "near", "due"]
        );
    }

    #[test]
    fn malformed_reminder_values_do_not_disable_valid_stored_thresholds_and_toggles() {
        let config = ReminderConfig::from_settings(|key| {
            Ok(match key {
                "reminder_warn_percent" => Some("82".into()),
                "reminder_near_percent" => Some("nope".into()),
                "reminder_due_percent" => Some("111".into()),
                "reminder_warn_sound" => Some("true".into()),
                "reminder_due_notify" => Some("false".into()),
                "reminder_overdue_interval_minutes" => Some("not an interval".into()),
                _ => None,
            })
        })
        .unwrap();
        assert_eq!(
            (config.warn_percent, config.near_percent, config.due_percent),
            (82, 90, 111)
        );
        assert!(config.warn_sound);
        assert!(!config.due_notify);
        assert_eq!(config.overdue_interval_minutes, 10);
    }

    #[test]
    fn out_of_range_settings_use_defaults_and_stage_percentages_are_sorted() {
        let config = ReminderConfig::from_settings(|key| {
            Ok(match key {
                "reminder_warn_percent" => Some("1200".into()),
                "reminder_near_percent" => Some("50".into()),
                "reminder_due_percent" => Some("25".into()),
                "reminder_overdue_interval_minutes" => Some("0".into()),
                _ => None,
            })
        })
        .unwrap();
        assert_eq!(
            (config.warn_percent, config.near_percent, config.due_percent),
            (25, 50, 75)
        );
        assert_eq!(config.overdue_interval_minutes, 10);
    }

    #[test]
    fn overdue_includes_all_relevant_stages_and_only_latest_reached_milestone() {
        let config = ReminderConfig::default();
        let due = due_reminders(1300, &config); // 1240 seconds overdue
        assert_eq!(
            due.iter().map(|r| r.node.as_str()).collect::<Vec<_>>(),
            ["warn", "near", "due", "overdue:at:1200"]
        );
        assert_eq!(due[3].level, ReminderLevel::Overdue);
        assert!(due[3].notify);
    }

    #[test]
    fn new_interval_includes_an_exact_edit_boundary_but_never_an_earlier_one() {
        let config = ReminderConfig {
            overdue_interval_minutes: 1,
            ..ReminderConfig::default()
        };
        let overdue_at = |edit, elapsed| {
            due_reminders(elapsed, &config.clone().for_task(60, Some(edit), None))
                .into_iter()
                .find(|r| r.level == ReminderLevel::Overdue)
                .map(|r| r.node)
        };
        assert_eq!(overdue_at(180, 182).as_deref(), Some("overdue:at:120"));
        assert_eq!(overdue_at(181, 182), None);
        assert_eq!(overdue_at(181, 245).as_deref(), Some("overdue:at:180"));
    }

    #[test]
    fn upgrade_starts_a_full_interval_and_keeps_due_relative_node_ids() {
        let config = ReminderConfig {
            overdue_interval_minutes: 1,
            ..ReminderConfig::default()
        };
        let overdue_at = |reset, edit, elapsed| {
            due_reminders(elapsed, &config.clone().for_task(60, edit, Some(reset)))
                .into_iter()
                .find(|r| r.level == ReminderLevel::Overdue)
                .map(|r| r.node)
        };
        assert_eq!(overdue_at(95, None, 154), None);
        assert_eq!(overdue_at(95, None, 155).as_deref(), Some("overdue:at:95"));
        assert_eq!(overdue_at(95, None, 277).as_deref(), Some("overdue:at:215"));
        assert_eq!(overdue_at(95, Some(156), 157), None);
        assert_eq!(
            overdue_at(95, Some(155), 155).as_deref(),
            Some("overdue:at:95")
        );
        assert_eq!(overdue_at(45, None, 119), None);
        assert_eq!(overdue_at(45, None, 120).as_deref(), Some("overdue:at:60"));
    }

    #[test]
    fn fractional_thresholds_floor_for_all_stages() {
        // Also exercised by the TypeScript visual/countdown tests.
        let config = ReminderConfig {
            est_seconds: 61,
            due_percent: 95,
            overdue_interval_minutes: 1,
            ..ReminderConfig::default()
        };
        for (elapsed, expected) in [
            (44, vec![]),
            (45, vec!["warn"]),
            (54, vec!["warn", "near"]),
            (56, vec!["warn", "near"]),
            (57, vec!["warn", "near", "due"]),
            (116, vec!["warn", "near", "due"]),
            (117, vec!["warn", "near", "due", "overdue:at:60"]),
        ] {
            let reminders = due_reminders(elapsed, &config);
            assert_eq!(
                reminders
                    .iter()
                    .map(|r| r.node.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
}
