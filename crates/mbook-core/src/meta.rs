//! The metadata of a book.

use crate::error::{MetaError, Result};

const TITLE: &str = "title";
const LANGUAGE: &str = "lang";
const ID: &str = "id";

/// How many rows the format allows for a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Count {
    One,
    OneOrMore,
    Optional,
    Any,
}

impl Count {
    /// The rule for a key. Keys the format does not define may repeat.
    fn of(key: &str) -> Self {
        match key {
            TITLE | ID => Self::One,
            LANGUAGE => Self::OneOrMore,
            "series" | "series-index" | "publisher" | "date" | "annotation" | "cover" => {
                Self::Optional
            }
            _ => Self::Any,
        }
    }

    fn is_required(self) -> bool {
        matches!(self, Self::One | Self::OneOrMore)
    }

    fn is_single(self) -> bool {
        matches!(self, Self::One | Self::Optional)
    }
}

/// The metadata of a book: its title, languages, authors and the rest.
///
/// A `Meta` always holds exactly one title, exactly one identifier and at
/// least one language, as the format requires; calls that would break this
/// return an error. Keys the format does not define, including private `x-`
/// keys, are kept and written back unchanged.
///
/// A `BookReader` gives read access to the metadata of a file, and a `Book`
/// gives access for changing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    /// Rows in file order; the order of rows with one key is significant.
    rows: Vec<(String, String)>,
}

/// The series a book belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Series<'a> {
    /// The name of the series.
    pub name: &'a str,
    /// The position in the series, a decimal number such as `2` or `2.5`.
    pub index: Option<&'a str>,
}

impl Meta {
    /// The metadata of a new book.
    #[cfg(any(feature = "write", test))]
    pub(crate) fn new(title: String, language: String, id: String) -> Result<Self, MetaError> {
        if title.is_empty() {
            return Err(MetaError::EmptyTitle);
        }
        Ok(Self {
            rows: vec![
                (TITLE.to_owned(), title),
                (LANGUAGE.to_owned(), language),
                (ID.to_owned(), id),
            ],
        })
    }

    /// The metadata stored in a file, checked against the rules of the
    /// format.
    #[cfg(any(feature = "read", test))]
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the reader calls it once it decodes tables")
    )]
    pub(crate) fn from_rows(rows: Vec<(String, String)>) -> Result<Self, MetaError> {
        let meta = Self { rows };
        for (key, _) in &meta.rows {
            check_key(key)?;
            if Count::of(key).is_single() && meta.get_all(key).nth(1).is_some() {
                return Err(MetaError::SingleValued(key.clone()));
            }
        }
        for key in [TITLE, LANGUAGE, ID] {
            if meta.get(key).is_none() {
                return Err(MetaError::Required(key.to_owned()));
            }
        }
        if meta.title().is_empty() {
            return Err(MetaError::EmptyTitle);
        }
        Ok(meta)
    }

    /// The title of the book.
    #[must_use]
    pub fn title(&self) -> &str {
        self.get(TITLE).unwrap_or_default()
    }

    /// The main language, a BCP 47 tag such as `en`.
    #[must_use]
    pub fn language(&self) -> &str {
        self.get(LANGUAGE).unwrap_or_default()
    }

    /// Every language of the book; the first is the main one.
    pub fn languages(&self) -> impl Iterator<Item = &str> {
        self.get_all(LANGUAGE)
    }

    /// The identifier that stays the same across re-packings of the book.
    #[must_use]
    pub fn id(&self) -> &str {
        self.get(ID).unwrap_or_default()
    }

    /// The authors, in the order they are credited.
    pub fn authors(&self) -> impl Iterator<Item = &str> {
        self.get_all("author")
    }

    /// The translators, in the order they are credited.
    pub fn translators(&self) -> impl Iterator<Item = &str> {
        self.get_all("translator")
    }

    /// The series the book belongs to.
    #[must_use]
    pub fn series(&self) -> Option<Series<'_>> {
        self.get("series").map(|name| Series {
            name,
            index: self.get("series-index"),
        })
    }

    /// The publisher.
    #[must_use]
    pub fn publisher(&self) -> Option<&str> {
        self.get("publisher")
    }

    /// The publication date: `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    #[must_use]
    pub fn date(&self) -> Option<&str> {
        self.get("date")
    }

    /// The ISBNs of the book.
    pub fn isbns(&self) -> impl Iterator<Item = &str> {
        self.get_all("isbn")
    }

    /// The genres or tags.
    pub fn genres(&self) -> impl Iterator<Item = &str> {
        self.get_all("genre")
    }

    /// A plain-text description; paragraphs are separated by an empty line.
    #[must_use]
    pub fn annotation(&self) -> Option<&str> {
        self.get("annotation")
    }

    /// The path of the cover picture.
    #[must_use]
    pub fn cover(&self) -> Option<&str> {
        self.get("cover")
    }

    /// The first value of any key.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "write")]
    /// # fn main() -> mbook_core::Result<()> {
    /// let mut book = mbook_core::Book::new("The Example", "en")?;
    /// book.meta_mut().set("x-shelf", "favourites")?;
    /// assert_eq!(book.meta().get("x-shelf"), Some("favourites"));
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "write"))]
    /// # fn main() {}
    /// ```
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.get_all(key).next()
    }

    /// Every value of a key, in order.
    pub fn get_all(&self, key: &str) -> impl Iterator<Item = &str> {
        self.rows
            .iter()
            .filter(move |(row_key, _)| row_key == key)
            .map(|(_, value)| value.as_str())
    }

    /// Every row as a key and a value, in file order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.rows
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    /// Sets the title.
    ///
    /// # Errors
    ///
    /// The title is empty.
    pub fn set_title(&mut self, title: impl Into<String>) -> Result<()> {
        self.set(TITLE, title)
    }

    /// Sets the language of the book, replacing every language it had.
    ///
    /// Add further languages with [`add`](Self::add) and the key `lang`.
    ///
    /// # Errors
    ///
    /// None today; the tag is not validated yet.
    pub fn set_language(&mut self, tag: impl Into<String>) -> Result<()> {
        self.set(LANGUAGE, tag)
    }

    /// Sets the identifier.
    ///
    /// # Errors
    ///
    /// None today; the identifier is not validated yet.
    pub fn set_id(&mut self, id: impl Into<String>) -> Result<()> {
        self.set(ID, id)
    }

    /// Adds an author after the ones already listed.
    ///
    /// # Errors
    ///
    /// None today; the name is not validated.
    pub fn add_author(&mut self, name: impl Into<String>) -> Result<()> {
        self.add("author", name)
    }

    /// Adds a translator after the ones already listed.
    ///
    /// # Errors
    ///
    /// None today; the name is not validated.
    pub fn add_translator(&mut self, name: impl Into<String>) -> Result<()> {
        self.add("translator", name)
    }

    /// Sets the series, and its index when one is given. Without an index the
    /// stored one is removed.
    ///
    /// # Errors
    ///
    /// None today; the values are not validated yet.
    pub fn set_series(&mut self, name: impl Into<String>, index: Option<&str>) -> Result<()> {
        self.set("series", name)?;
        match index {
            Some(index) => self.set("series-index", index),
            None => self.remove("series-index"),
        }
    }

    /// Sets the publisher.
    ///
    /// # Errors
    ///
    /// None today; the name is not validated.
    pub fn set_publisher(&mut self, name: impl Into<String>) -> Result<()> {
        self.set("publisher", name)
    }

    /// Sets the publication date: `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    ///
    /// # Errors
    ///
    /// None today; the date is not validated yet.
    pub fn set_date(&mut self, date: impl Into<String>) -> Result<()> {
        self.set("date", date)
    }

    /// Adds an ISBN.
    ///
    /// # Errors
    ///
    /// None today; the number is not validated yet.
    pub fn add_isbn(&mut self, isbn: impl Into<String>) -> Result<()> {
        self.add("isbn", isbn)
    }

    /// Adds a genre or tag.
    ///
    /// # Errors
    ///
    /// None today; the value is not validated.
    pub fn add_genre(&mut self, genre: impl Into<String>) -> Result<()> {
        self.add("genre", genre)
    }

    /// Sets the plain-text description.
    ///
    /// # Errors
    ///
    /// None today; the text is not validated.
    pub fn set_annotation(&mut self, text: impl Into<String>) -> Result<()> {
        self.set("annotation", text)
    }

    /// Names the cover by the path of a picture.
    ///
    /// Whether the path names a picture of the book is checked when the book
    /// is written.
    ///
    /// # Errors
    ///
    /// None today; the path is checked when the book is written.
    pub fn set_cover(&mut self, path: impl Into<String>) -> Result<()> {
        self.set("cover", path)
    }

    /// Replaces every row of a key with one row.
    ///
    /// # Errors
    ///
    /// The key is not valid, or the key is `title` and the value is empty.
    pub fn set(&mut self, key: &str, value: impl Into<String>) -> Result<()> {
        let value = checked(key, value.into())?;
        match self.rows.iter().position(|(row_key, _)| row_key == key) {
            Some(first) => {
                self.rows[first].1 = value;
                let mut seen = 0;
                self.rows.retain(|(row_key, _)| {
                    seen += usize::from(row_key == key);
                    row_key != key || seen == 1
                });
            }
            None => self.rows.push((key.to_owned(), value)),
        }
        Ok(())
    }

    /// Adds a row after the ones the key already has.
    ///
    /// # Errors
    ///
    /// The key is not valid, or it holds one value and already has it.
    pub fn add(&mut self, key: &str, value: impl Into<String>) -> Result<()> {
        let value = checked(key, value.into())?;
        if Count::of(key).is_single() && self.get(key).is_some() {
            return Err(MetaError::SingleValued(key.to_owned()).into());
        }
        self.rows.push((key.to_owned(), value));
        Ok(())
    }

    /// Removes every row of a key. Removing a key that is absent succeeds.
    ///
    /// # Errors
    ///
    /// The key is `title`, `lang` or `id`.
    pub fn remove(&mut self, key: &str) -> Result<()> {
        if Count::of(key).is_required() {
            return Err(MetaError::Required(key.to_owned()).into());
        }
        self.rows.retain(|(row_key, _)| row_key != key);
        Ok(())
    }
}

/// Checks a key and the value given for it.
fn checked(key: &str, value: String) -> Result<String, MetaError> {
    check_key(key)?;
    if key == TITLE && value.is_empty() {
        return Err(MetaError::EmptyTitle);
    }
    Ok(value)
}

/// A key matches `[a-z0-9]+(-[a-z0-9]+)*`.
fn check_key(key: &str) -> Result<(), MetaError> {
    let valid = key.split('-').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    });
    if valid {
        Ok(())
    } else {
        Err(MetaError::InvalidKey(key.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::Meta;
    use crate::error::{Error, MetaError};

    fn meta() -> Meta {
        Meta::new("The Example".into(), "en".into(), "urn:uuid:0".into()).unwrap()
    }

    fn rows(rows: &[(&str, &str)]) -> Vec<(String, String)> {
        rows.iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn meta_error(result: crate::Result<()>) -> MetaError {
        match result {
            Err(Error::InvalidMeta(error)) => error,
            other => panic!("expected a metadata error, got {other:?}"),
        }
    }

    #[test]
    fn new_meta_has_the_three_required_keys() {
        let meta = meta();
        assert_eq!(
            (meta.title(), meta.language(), meta.id()),
            ("The Example", "en", "urn:uuid:0")
        );
    }

    #[test]
    fn new_rejects_an_empty_title() {
        let result = Meta::new(String::new(), "en".into(), "id".into());
        assert_eq!(result, Err(MetaError::EmptyTitle));
    }

    #[test]
    fn add_keeps_the_order_of_authors() {
        let mut meta = meta();
        meta.add_author("A. Writer").unwrap();
        meta.add_author("B. Editor").unwrap();
        assert_eq!(
            meta.authors().collect::<Vec<_>>(),
            ["A. Writer", "B. Editor"]
        );
    }

    #[test]
    fn set_replaces_every_row_of_the_key_with_one() {
        let mut meta = meta();
        meta.add("lang", "fr").unwrap();
        meta.set_language("de").unwrap();
        assert_eq!(meta.languages().collect::<Vec<_>>(), ["de"]);
    }

    #[test]
    fn set_keeps_the_position_of_the_first_row() {
        let mut meta = meta();
        meta.set_title("Renamed").unwrap();
        assert_eq!(meta.iter().next(), Some(("title", "Renamed")));
    }

    #[test]
    fn set_rejects_an_empty_title() {
        assert_eq!(meta_error(meta().set_title("")), MetaError::EmptyTitle);
    }

    #[test]
    fn add_rejects_a_second_value_for_a_single_key() {
        let mut meta = meta();
        meta.set_publisher("First").unwrap();
        let error = meta_error(meta.add("publisher", "Second"));
        assert_eq!(error, MetaError::SingleValued("publisher".into()));
    }

    #[test]
    fn remove_rejects_a_required_key() {
        let error = meta_error(meta().remove("id"));
        assert_eq!(error, MetaError::Required("id".into()));
    }

    #[test]
    fn remove_succeeds_for_an_absent_key() {
        assert!(meta().remove("publisher").is_ok());
    }

    #[test]
    fn set_series_without_an_index_removes_the_stored_index() {
        let mut meta = meta();
        meta.set_series("Saga", Some("2")).unwrap();
        meta.set_series("Saga", None).unwrap();
        assert_eq!(meta.series().and_then(|series| series.index), None);
    }

    #[test]
    fn unknown_keys_may_repeat() {
        let mut meta = meta();
        meta.add("x-shelf", "a").unwrap();
        meta.add("x-shelf", "b").unwrap();
        assert_eq!(meta.get_all("x-shelf").count(), 2);
    }

    #[test]
    fn set_rejects_a_key_with_an_uppercase_letter() {
        let error = meta_error(meta().set("Title", "x"));
        assert_eq!(error, MetaError::InvalidKey("Title".into()));
    }

    #[test]
    fn set_rejects_a_key_with_an_empty_part() {
        let error = meta_error(meta().set("x--shelf", "x"));
        assert_eq!(error, MetaError::InvalidKey("x--shelf".into()));
    }

    #[test]
    fn from_rows_accepts_a_complete_table() {
        let table = rows(&[("title", "T"), ("lang", "en"), ("id", "i"), ("author", "A")]);
        assert!(Meta::from_rows(table).is_ok());
    }

    #[test]
    fn from_rows_rejects_a_missing_identifier() {
        let table = rows(&[("title", "T"), ("lang", "en")]);
        assert_eq!(
            Meta::from_rows(table),
            Err(MetaError::Required("id".into()))
        );
    }

    #[test]
    fn from_rows_rejects_two_titles() {
        let table = rows(&[("title", "T"), ("title", "U"), ("lang", "en"), ("id", "i")]);
        assert_eq!(
            Meta::from_rows(table),
            Err(MetaError::SingleValued("title".into()))
        );
    }

    #[test]
    fn from_rows_rejects_an_empty_title() {
        let table = rows(&[("title", ""), ("lang", "en"), ("id", "i")]);
        assert_eq!(Meta::from_rows(table), Err(MetaError::EmptyTitle));
    }
}
