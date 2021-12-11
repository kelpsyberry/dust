#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RangeInclusive<Idx> {
    pub start: Idx,
    pub end: Idx,
}

impl<Idx> RangeInclusive<Idx> {
    #[inline]
    pub fn contains<U>(&self, item: &U) -> bool
    where
        U: ?Sized + PartialOrd<Idx>,
        Idx: PartialOrd<U>,
    {
        *item >= self.start && *item <= self.end
    }
}

impl<Idx> RangeInclusive<Idx>
where
    Idx: PartialOrd,
{
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.end < self.start
    }
}

impl<Idx> From<(Idx, Idx)> for RangeInclusive<Idx> {
    #[inline]
    fn from((start, end): (Idx, Idx)) -> Self {
        RangeInclusive { start, end }
    }
}

impl<Idx> From<RangeInclusive<Idx>> for (Idx, Idx) {
    #[inline]
    fn from(range: RangeInclusive<Idx>) -> Self {
        (range.start, range.end)
    }
}

impl<Idx> IntoIterator for RangeInclusive<Idx>
where
    Idx: core::iter::Step,
{
    type IntoIter = core::ops::RangeInclusive<Idx>;
    type Item = Idx;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.start..=self.end
    }
}
