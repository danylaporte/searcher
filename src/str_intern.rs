#[derive(Debug)]
pub(crate) struct StrIntern(Vec<Box<str>>);

impl StrIntern {
    pub(crate) const fn new() -> Self {
        Self(Vec::new())
    }

    fn binary_search(&self, s: &str) -> Result<usize, usize> {
        self.0.binary_search_by_key(&s, |r| r)
    }

    pub(crate) fn insert(&mut self, s: &str) -> &'static str {
        let index = match self.binary_search(s) {
            Ok(index) => index,
            Err(index) => {
                self.0.insert(index, s.into());
                index
            }
        };

        unsafe {
            // because it is a string interner, we assume the user
            // will manage correctly the lifetime of the string.
            let ptr: *const str = &**self.0.get_unchecked(index);
            &*ptr
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// Remove and deallocate the string. Make sure that the string is not referenced before removed it.
    pub(crate) fn remove(&mut self, s: &str) {
        if let Ok(index) = self.binary_search(s) {
            self.0.remove(index);
        }
    }
}

impl Default for StrIntern {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq<Vec<&str>> for StrIntern {
    fn eq(&self, other: &Vec<&str>) -> bool {
        self.0.iter().map(|b| &**b).eq(other.iter().map(|s| &**s))
    }
}

impl PartialEq<Vec<&str>> for &StrIntern {
    fn eq(&self, other: &Vec<&str>) -> bool {
        self.0.iter().map(|b| &**b).eq(other.iter().map(|s| &**s))
    }
}
