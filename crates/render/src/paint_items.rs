use havi_types::Fragment;

pub(crate) enum PaintSource<'a> {
    Direct(&'a Fragment),
    Hoisted(&'a Fragment),
}

impl<'a> PaintSource<'a> {
    pub(crate) fn fragment(&self) -> &'a Fragment {
        match self {
            Self::Direct(fragment) | Self::Hoisted(fragment) => fragment,
        }
    }
}
