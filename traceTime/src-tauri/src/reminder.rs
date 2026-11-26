use serde::{Deserialize, Serialize};

const DEFAULT_EST_SECONDS: i64 = 60;
const DEFAULT_OVERDUE_INTERVAL_MINUTES: i64 = 10;
pub const OVERDUE_INTERVAL_KEY: &str = "reminder_overdue_interval_minutes";

pub fn parse_overdue_interval(value: Option<&str>) -> Result<i64, String> {
    match value {
        None => Ok(DEFAULT_OVERDUE_INTERVAL_MINUTES),
        Some(value) => value
            .parse::<i64>()
            .map(|minutes| minutes.clamp(1, 1440))
            .map_err(|_| format!("invalid integer setting {OVERDUE_INTERVAL_KEY}: {value}")),
    }
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
    overdue_from_net: Option<i64>,
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
            overdue_from_net: None,
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
            match get(key)? {
                None => Ok(default),
                Some(value) => match value.as_str() {
                    "true" | "1" => Ok(true),
                    "false" | "0" => Ok(false),
                    _ => Err(format!("invalid boolean setting {key}: {value}")),
                },
            }
        }

        fn int_value(
            get: &impl Fn(&str) -> Result<Option<String>, String>,
            key: &str,
            default: i64,
            range: std::ops::RangeInclusive<i64>,
        ) -> Result<i64, String> {
            let value = match get(key)? {
                None => default,
                Some(value) => value
                    .parse::<i64>()
                    .map_err(|_| format!("invalid integer setting {key}: {value}"))?,
            };
            Ok(value.clamp(*range.start(), *range.end()))
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
            overdue_interval_minutes: parse_overdue_interval(
                get(OVERDUE_INTERVAL_KEY)?.as_deref(),
            )?,
            warn_sound: bool_value(&get, "reminder_warn_sound", defaults.warn_sound)?,
            near_sound: bool_value(&get, "reminder_near_sound", defaults.near_sound)?,
            due_notify: bool_value(&get, "reminder_due_notify", defaults.due_notify)?,
            ..defaults
        })
    }

    pub(crate) fn for_task(mut self, est_seconds: i64, overdue_from_net: Option<i64>) -> Self {
        self.est_seconds = est_seconds;
        self.overdue_from_net = overdue_from_net;
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
    // Round the schedule's lower bound (upgrade or interval change) up to a
    // repeat boundary. A late poll emits only the newest reached boundary.
    let interval = i128::from(config.overdue_interval_minutes) * 60;
    let overtime = |net: i64| (i128::from(net) - i128::from(due_at)).max(0);
    let current_overtime = overtime(net_seconds);
    let first_milestone = config.overdue_from_net.map_or(interval, |from_net| {
        ((overtime(from_net) + interval - 1) / interval * interval).max(interval)
    });
    let reached_milestone = current_overtime / interval * interval;
    if config.overdue_enabled && reached_milestone >= first_milestone {
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
    fn settings_are_clamped_and_stage_percentages_are_sorted() {
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
            (25, 50, 1000)
        );
        assert_eq!(config.overdue_interval_minutes, 1);
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
            due_reminders(elapsed, &config.clone().for_task(60, Some(edit)))
                .into_iter()
                .find(|r| r.level == ReminderLevel::Overdue)
                .map(|r| r.node)
        };
        assert_eq!(overdue_at(180, 182).as_deref(), Some("overdue:at:120"));
        assert_eq!(overdue_at(181, 182), None);
        assert_eq!(overdue_at(181, 245).as_deref(), Some("overdue:at:180"));
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
