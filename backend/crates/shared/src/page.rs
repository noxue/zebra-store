//! Pagination parameters and metadata.

use serde::{Deserialize, Serialize};

/// Default page size when the client does not send one.
pub const DEFAULT_PAGE_SIZE: u64 = 20;
/// Largest accepted page size (matches the original `ginutil` pagination).
pub const MAX_PAGE_SIZE: u64 = 200;

/// A normalized page request (1-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    pub page: u64,
    pub page_size: u64,
}

impl PageRequest {
    /// Normalizes raw values: page >= 1, 1 <= page_size <= [`MAX_PAGE_SIZE`].
    pub fn new(page: Option<u64>, page_size: Option<u64>) -> Self {
        let page = page.filter(|p| *p > 0).unwrap_or(1);
        let page_size = match page_size {
            Some(0) | None => DEFAULT_PAGE_SIZE,
            Some(size) => size.min(MAX_PAGE_SIZE),
        };
        Self { page, page_size }
    }

    pub fn offset(self) -> u64 {
        (self.page - 1) * self.page_size
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Pagination metadata returned in the response envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u64,
    pub page_size: u64,
    pub total: u64,
    pub total_page: u64,
}

impl Pagination {
    pub fn new(req: PageRequest, total: u64) -> Self {
        Self {
            page: req.page,
            page_size: req.page_size,
            total,
            total_page: total.div_ceil(req.page_size.max(1)),
        }
    }
}

/// A page of items plus its total count.
#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
}

impl<T> Page<T> {
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_page_size() {
        let req = PageRequest::new(Some(0), Some(999));
        assert_eq!(
            req,
            PageRequest {
                page: 1,
                page_size: 200
            }
        );
        assert_eq!(PageRequest::new(Some(3), Some(10)).offset(), 20);
    }

    #[test]
    fn computes_total_pages() {
        let p = Pagination::new(PageRequest::new(Some(1), Some(10)), 21);
        assert_eq!(p.total_page, 3);
        assert_eq!(Pagination::new(PageRequest::default(), 0).total_page, 0);
    }
}
