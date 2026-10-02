use std::fmt::Display;
use std::io::BufRead;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use simplelog::info;

use crate::tracker::config::Side;

const RESET_INPUT: &str = "-";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SideProject<T> {
    pub side_num: u8,
    pub project_id: T,
}

pub fn project_for_side<'a, T>(
    side_projects: &'a [SideProject<T>],
    side_num: u8,
    default: &'a T,
) -> &'a T {
    side_projects
        .iter()
        .find(|p| p.side_num == side_num)
        .map(|p| &p.project_id)
        .unwrap_or(default)
}

pub fn set_side_project<T: PartialEq>(
    side_projects: &mut Vec<SideProject<T>>,
    side_num: u8,
    project_id: Option<T>,
) -> bool {
    let existing = side_projects.iter().position(|p| p.side_num == side_num);

    match (existing, project_id) {
        (Some(idx), None) => {
            side_projects.remove(idx);
            true
        }
        (Some(idx), Some(project_id)) => {
            if side_projects[idx].project_id == project_id {
                return false;
            }
            side_projects[idx].project_id = project_id;
            true
        }
        (None, Some(project_id)) => {
            side_projects.push(SideProject {
                side_num,
                project_id,
            });
            true
        }
        (None, None) => false,
    }
}

/// Asks for a project id for every labeled side. Returns true if anything changed.
pub fn prompt_side_projects<T: FromStr + Display + PartialEq>(
    service: &str,
    sides: &[Side],
    side_projects: &mut Vec<SideProject<T>>,
) -> bool {
    prompt_side_projects_from(&mut std::io::stdin().lock(), service, sides, side_projects)
}

fn prompt_side_projects_from<R: BufRead, T: FromStr + Display + PartialEq>(
    reader: &mut R,
    service: &str,
    sides: &[Side],
    side_projects: &mut Vec<SideProject<T>>,
) -> bool {
    let mut changed = false;

    for side in sides.iter().filter(|s| !s.label.is_empty()) {
        loop {
            let mut message = format!(
                "Provide {} project id for side {} ({}), leave blank to skip",
                service, side.side_num, side.label
            );
            if let Some(current) = side_projects.iter().find(|p| p.side_num == side.side_num) {
                message.push_str(
                    format!(
                        "\ncurrent value {}, enter \"{}\" to use the default project",
                        current.project_id, RESET_INPUT
                    )
                    .as_str(),
                );
            }
            info!("{message}");

            let mut input = String::new();
            if reader.read_line(&mut input).unwrap_or(0) == 0 {
                return changed;
            }
            let input = input.trim();

            if input.is_empty() {
                break;
            }

            if input == RESET_INPUT {
                changed |= set_side_project(side_projects, side.side_num, None);
                break;
            }

            match input.parse::<T>() {
                Ok(project_id) => {
                    changed |= set_side_project(side_projects, side.side_num, Some(project_id));
                    break;
                }
                Err(_) => info!("Invalid project id \"{}\", try again", input),
            }
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(side_num: u8, label: &str) -> Side {
        Side {
            side_num,
            label: label.to_string(),
            configurable: true,
        }
    }

    #[test]
    fn falls_back_to_default_project() {
        let projects = vec![SideProject {
            side_num: 1,
            project_id: 10u64,
        }];

        assert_eq!(*project_for_side(&projects, 1, &5), 10);
        assert_eq!(*project_for_side(&projects, 2, &5), 5);
    }

    #[test]
    fn set_side_project_adds_updates_and_removes() {
        let mut projects: Vec<SideProject<u64>> = vec![];

        assert!(set_side_project(&mut projects, 1, Some(10)));
        assert!(!set_side_project(&mut projects, 1, Some(10)));
        assert!(set_side_project(&mut projects, 1, Some(11)));
        assert_eq!(projects[0].project_id, 11);
        assert!(set_side_project(&mut projects, 1, None));
        assert!(projects.is_empty());
        assert!(!set_side_project(&mut projects, 1, None));
    }

    #[test]
    fn prompt_skips_unlabeled_sides_and_handles_input() {
        let sides = vec![side(1, "a"), side(2, ""), side(3, "c"), side(4, "d")];
        let mut projects = vec![
            SideProject {
                side_num: 3,
                project_id: 30u64,
            },
            SideProject {
                side_num: 4,
                project_id: 40u64,
            },
        ];
        // side 1: invalid then 10, side 3: blank keeps 30, side 4: reset
        let mut input = "abc\n10\n\n-\n".as_bytes();

        let changed = prompt_side_projects_from(&mut input, "Test", &sides, &mut projects);

        assert!(changed);
        assert_eq!(
            projects,
            vec![
                SideProject {
                    side_num: 3,
                    project_id: 30
                },
                SideProject {
                    side_num: 1,
                    project_id: 10
                },
            ]
        );
    }

    #[test]
    fn serializes_to_toml_with_other_fields() {
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct Cfg {
            project_id: String,
            #[serde(default)]
            side_projects: Vec<SideProject<String>>,
            workspace_id: String,
        }

        let legacy: Cfg = toml::from_str("project_id = \"p\"\nworkspace_id = \"w\"").unwrap();
        assert!(legacy.side_projects.is_empty());

        let cfg = Cfg {
            project_id: "p".into(),
            side_projects: vec![SideProject {
                side_num: 2,
                project_id: "x".into(),
            }],
            workspace_id: "w".into(),
        };
        let mut table = toml::value::Table::new();
        table.insert("svc".into(), toml::Value::try_from(&cfg).unwrap());
        let text = toml::to_string(&table).unwrap();
        let parsed: toml::value::Table = toml::from_str(&text).unwrap();
        let round: Cfg = parsed["svc"].clone().try_into().unwrap();
        assert_eq!(round, cfg);
    }
}
