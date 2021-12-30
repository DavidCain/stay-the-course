use serde_derive::Deserialize;

use chrono::NaiveDate;
use std::fs;

#[derive(Deserialize)]
struct User {
    birthday: String, // YYYY-MM-DD
}

impl User {
    fn birthday(&self) -> NaiveDate {
        NaiveDate::parse_from_str(&self.birthday, "%Y-%m-%d").unwrap()
    }
}

#[derive(Deserialize)]
pub struct GnuCash {
    pub path_to_book: String,
    pub file_format: String,
    pub update_prices: bool,
}

#[derive(Deserialize)]
pub struct Config {
    user: User,
    pub gnucash: GnuCash,
}

impl Config {
    /// Return default settings for use with the sample data
    pub fn default() -> Config {
        Config {
            user: User {
                birthday: String::from("1985-01-01"),
            },
            gnucash: GnuCash {
                path_to_book: String::from("example/sqlite3.gnucash"),
                file_format: String::from("sqlite3"),
                // This requires GnuCash to be installed.
                // So that people can demo with *just* Rust, assume it's off by default.
                update_prices: false,
            },
        }
    }

    pub fn user_birthday(&self) -> NaiveDate {
        self.user.birthday()
    }

    /// Return a Config from file, or default settings if not present
    ///
    /// See `example_config.toml` for a sample configuration:
    ///
    /// ```toml
    /// [user]
    /// birthday = '1971-06-14'
    ///
    /// [gnucash]
    /// path_to_book = '/path/to/database.gnucash'
    /// file_format = 'sqlite3'
    /// ```
    pub fn from_file(path: &str) -> Config {
        let config_toml = match fs::read_to_string(path) {
            Ok(file) => file,
            Err(_) => {
                // Silently fall back to the default
                return Config::default();
            }
        };

        toml::from_str(&config_toml).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parses_birthday() {
        let user = User {
            birthday: String::from("1962-12-31"),
        };
        assert_eq!(user.birthday(), NaiveDate::from_ymd(1962, 12, 31));
    }

    #[test]
    fn test_parse_from_toml() {
        let conf = Config::from_file("example_config.toml");
        assert_eq!(conf.user_birthday(), NaiveDate::from_ymd(1972, 7, 12));
        assert_eq!(&conf.gnucash.path_to_book, "/home/linus/sqlite3.gnucash");
        assert_eq!(&conf.gnucash.file_format, "sqlite3");
        assert_eq!(conf.gnucash.update_prices, true);
    }

    #[test]
    fn test_fallback_to_default_settings() {
        let conf = Config::from_file("/tmp/definitely_does_not_exist.toml");
        assert_eq!(&conf.user.birthday, "1985-01-01");
        assert_eq!(&conf.gnucash.path_to_book, "example/sqlite3.gnucash");
        assert_eq!(&conf.gnucash.file_format, "sqlite3");
        assert_eq!(conf.gnucash.update_prices, false);
    }
}
