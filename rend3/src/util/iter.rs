/// Iterator adapter that implements ExactSizeIterator even
/// when the inner iterator doesn't. Size must be accurate
/// for the count of the iterator.
#[derive(Clone)]
pub struct ExactSizerIterator<I> {
    inner: I,
    size: usize,
}

impl<I> ExactSizerIterator<I> where I: Iterator + Clone {
    /// Creates a new iterator adapter. In debug validates
    /// that the iterator is the same length as it says it is.
    pub fn new(inner: I, size: usize) -> Self {
        debug_assert_eq!(inner.clone().count(), size);
        Self {
            inner,
            size,
        }
    }
 }

impl<I> Iterator for ExactSizerIterator<I>
where
    I: Iterator,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.size, Some(self.size))
    }
}

impl<I> ExactSizeIterator for ExactSizerIterator<I>
where
    I: Iterator,
{
    fn len(&self) -> usize {
        let (lower, _) = self.size_hint();
        lower
    }
}
