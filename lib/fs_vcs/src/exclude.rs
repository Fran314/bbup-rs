use abst_fs::AbstPath;
use regex::Regex;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ExcludeListError {
    #[error("Exclude List Error: Failed to parse rule to regex\nrule: {rule}\nreason: {err}")]
    UnparsableRule { rule: String, err: regex::Error },
}
fn unparerr<S: std::string::ToString>(rule: S) -> impl Fn(regex::Error) -> ExcludeListError {
    move |err: regex::Error| -> ExcludeListError {
        ExcludeListError::UnparsableRule {
            rule: rule.to_string(),
            err,
        }
    }
}

pub struct ExcludeList(Vec<Regex>);

impl ExcludeList {
    pub fn from(rules: &Vec<String>) -> Result<ExcludeList, ExcludeListError> {
        ExcludeList(vec![Regex::new("\\.bbup/").unwrap()]).join(rules)
    }
    pub fn join(self, rules: &Vec<String>) -> Result<ExcludeList, ExcludeListError> {
        let ExcludeList(mut list) = self;
        for rule in rules {
            let rgx = Regex::new(rule).map_err(unparerr(rule))?;
            list.push(rgx);
        }
        Ok(ExcludeList(list))
    }

    // TODO this should somehow implement the blob pattern thingy
    pub fn should_exclude(&self, path: &AbstPath, is_dir: bool) -> bool {
        let ExcludeList(list) = self;
        let path_as_string = {
            let mut tmp = path.to_string();
            if is_dir {
                tmp.push('/');
            }
            tmp
        };

        for rule in list {
            if rule.is_match(path_as_string.as_str()) {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::{unparerr, ExcludeList, ExcludeListError};
    use abst_fs::AbstPath;
    use regex::Regex;

    fn assert_lists_eq(
        ExcludeList(list_a): ExcludeList,
        ExcludeList(list_b): ExcludeList,
    ) -> Result<(), Box<dyn std::any::Any + Send>> {
        // We wrap the assert in a catch_unwind and return a result so that
        // later we can be sure on which unwrapped call of this function failed
        // the assertion
        std::panic::catch_unwind(|| {
            assert_eq!(
                list_a
                    .into_iter()
                    .map(|rule| rule.to_string())
                    .collect::<Vec<String>>(),
                list_b
                    .into_iter()
                    .map(|rule| rule.to_string())
                    .collect::<Vec<String>>()
            )
        })
    }

    #[test]
    fn test_error() {
        let invalid_rule = "BOOM\\";
        let err = Regex::new(invalid_rule).unwrap_err();
        assert_eq!(
            unparerr(invalid_rule)(err.clone()),
            ExcludeListError::UnparsableRule {
                rule: invalid_rule.to_string(),
                err
            }
        )
    }

    #[test]
    fn test_from() {
        assert_lists_eq(
            ExcludeList::from(&vec![]).unwrap(),
            ExcludeList(vec![Regex::new("\\.bbup/").unwrap()]),
        )
        .unwrap();

        assert_lists_eq(
            ExcludeList::from(&vec![
                String::from("^prova$"),
                String::from("[0-9]?[0-9]:[0-9][0-9]"),
                String::from("^[a-zA-Z0-9 ]*$"),
            ])
            .unwrap(),
            ExcludeList(vec![
                Regex::new("\\.bbup/").unwrap(),
                Regex::new("^prova$").unwrap(),
                Regex::new("[0-9]?[0-9]:[0-9][0-9]").unwrap(),
                Regex::new("^[a-zA-Z0-9 ]*$").unwrap(),
            ]),
        )
        .unwrap();
    }

    #[test]
    fn test_join() {
        assert_lists_eq(
            ExcludeList::from(&vec![String::from("[0-9]?[0-9]:[0-9][0-9]")])
                .unwrap()
                .join(&vec![
                    String::from("^prova$"),
                    String::from("^[a-zA-Z0-9 ]*$"),
                ])
                .unwrap(),
            ExcludeList(vec![
                Regex::new("\\.bbup/").unwrap(),
                Regex::new("[0-9]?[0-9]:[0-9][0-9]").unwrap(),
                Regex::new("^prova$").unwrap(),
                Regex::new("^[a-zA-Z0-9 ]*$").unwrap(),
            ]),
        )
        .unwrap();
    }

    #[test]
    fn test_should_exclude() {
        let exclude_list = ExcludeList::from(&vec![
            String::from("some-directory/"),
            String::from("some-name"),
            String::from("\\./root-file"),
        ])
        .unwrap();

        assert!(exclude_list.should_exclude(&AbstPath::from("./some-directory"), true));
        assert!(!exclude_list.should_exclude(&AbstPath::from("./some-directory"), false));

        assert!(exclude_list.should_exclude(&AbstPath::from("./some-name"), true));
        assert!(exclude_list.should_exclude(&AbstPath::from("./some-name"), false));
        assert!(exclude_list.should_exclude(&AbstPath::from("./path/to/some-name"), true));
        assert!(exclude_list.should_exclude(&AbstPath::from("./path/to/some-name"), false));

        assert!(exclude_list.should_exclude(&AbstPath::from("./root-file"), true));
        assert!(exclude_list.should_exclude(&AbstPath::from("./root-file"), false));
        assert!(!exclude_list.should_exclude(&AbstPath::from("./path/to/root-file"), true));
        assert!(!exclude_list.should_exclude(&AbstPath::from("./path/to/root-file"), false));
    }
}
