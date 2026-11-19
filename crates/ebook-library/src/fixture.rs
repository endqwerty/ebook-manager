//! Generate a browseable test catalogue. Only its final change has a portable file.
use crate::*;
pub fn id(i: u32) -> String {
    Uuid::from_u128(u128::from(i) + 1).to_string()
}
pub fn bucket(i: u32) -> u32 {
    i.wrapping_mul(17) % 4096
}
pub fn title(i: u32) -> String {
    let k = bucket(i);
    let suffix = match k % 4 {
        0 => "A distant atlas",
        1 => "海辺",
        2 => "Письма",
        _ => "Straße",
    };
    format!("Book {k} {suffix}")
}
pub fn generate(path: &Path, count: u32) -> Result<()> {
    ensure!(
        count > 0 && !path.join("library.sqlite").exists(),
        "fixture requires an empty catalogue"
    );
    let mut lib = Library::open(path)?;
    let mut parent = None;
    for start in (0..count).step_by(8192) {
        let tx = lib.db.transaction()?;
        for i in start..(start + 8192).min(count) {
            let identity = id(i);
            let title = title(i);
            let authors = vec![format!("Author {}", i % 257)];
            let m = ParsedMetadata {
                title: title.clone(),
                authors: authors.clone(),
                identifier: Some(format!("package-{i}")),
                series: if i % 3 == 0 {
                    vec![("Cycle".into(), Some((i % 17).to_string()))]
                } else {
                    vec![]
                },
                published: (i % 5 != 0).then(|| format!("{}", 1900 + i % 100)),
                survey: Some(SurveyFacts {
                    version: 1,
                    words: Some(i % 480000),
                    cover: None,
                }),
                ..Default::default()
            };
            let b = Book {
                id: identity.clone(),
                edition_id: identity.clone(),
                file_id: identity.clone(),
                version_id: identity.clone(),
                sha256: format!("{i:064x}"),
                title,
                authors,
                imported: m,
                added: Some(i64::from(i % 7)),
                sort_title: None,
                author_sorts: None,
                series: None,
            };
            let entry = Entry {
                format: 1,
                id: identity.clone(),
                parent,
                change: Change::Book(Box::new(b)),
            };
            let data = serde_json::to_string(&entry)?;
            journal::append(&tx, &entry, &data, &lib.keys)?;
            tx.prepare_cached("INSERT INTO sources VALUES(?,?)")?
                .execute(params![format!("source-{i}").as_bytes(), identity])?;
            if i + 1 == count {
                install(
                    &lib.portable.join(format!("changes/{count:020}.json")),
                    data.as_bytes(),
                )?;
            }
            parent = Some(identity);
        }
        tx.execute("UPDATE journal SET published=1 WHERE seq>?", [start])?;
        tx.execute(
            "UPDATE meta SET value=? WHERE key='tip'",
            [(start + 8192).min(count)],
        )?;
        tx.commit()?;
    }
    Ok(())
}
pub fn lookup(lib: &Library, i: u32) -> Result<Vec<Book>> {
    Ok(lib
        .import_candidates(
            format!("source-{i}").as_bytes(),
            &format!("{i:064x}"),
            Some(&format!("package-{i}")),
        )?
        .1)
}
