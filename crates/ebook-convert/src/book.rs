//! Values crossing from a format decoder to the EPUB writer inside the helper.
#[derive(Debug, Default)]
pub struct ParsedBook {
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub sections: Vec<Section>,
    pub images: Vec<Image>,
    pub cover: Option<usize>,
    pub losses: Vec<String>,
}

#[derive(Debug)]
pub struct Section {
    pub title: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug)]
pub enum Block {
    Paragraph(String),
    Heading(String),
    Image(usize),
}

#[derive(Debug)]
pub struct Image {
    pub kind: ImageKind,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub enum ImageKind {
    Jpeg,
    Png,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    Protected,
    Unsupported(String),
    Invalid(String),
    Limit,
}
