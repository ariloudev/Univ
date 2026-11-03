//! What a publish or prune tells as it goes, and how the owner stops it after the unit in hand.

/// A file a publish has done: uploaded to R2 and checked, or executed against D1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDone<'a> {
    /// Its path in the release directory.
    pub path: &'a str,
    /// Its size, the bytes uploaded.
    pub bytes: u64,
    /// What its line says: how it was checked, or that it was imported and how its table was
    /// checked once it was the table's last file.
    pub line: String,
    /// The table it loads and the rows of the release the table holds, once the table's last
    /// file is imported and the table checked by its rows.
    pub rows: Option<(&'static str, u64)>,
}

/// What a publish or prune tells as it goes.
pub trait Progress {
    /// The run has `items` to do: the files it will upload, or the steps of a prune.
    fn started(&mut self, items: usize) {
        let _ = items;
    }
    /// A step that is no file, such as the files a journal already lists.
    fn line(&mut self, line: String);
    /// A file done, as soon as it is.
    fn file(&mut self, file: FileDone<'_>) {
        self.line(file.line);
    }
    /// Whether the owner asked the run to stop once the unit in hand is done.
    fn stopping(&self) -> bool {
        false
    }
}

/// Each line given to a closure, and never asked to stop.
pub struct Lines<'a>(pub &'a mut dyn FnMut(String));

impl Progress for Lines<'_> {
    fn line(&mut self, line: String) {
        (self.0)(line);
    }
}
