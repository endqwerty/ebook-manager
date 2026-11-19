INSERT INTO book(
    id,sha256,identifier,data,title,authors,author_sort,series,position,title_key,author_key,series_key,position_key,added,published,words,language,spine_rgb,cover_rgb,cover_digest,version_id,rtl,survey_version)

VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
ON CONFLICT(id) DO UPDATE SET
    sha256=excluded.sha256,
    identifier=excluded.identifier,data=excluded.data,title=excluded.title,authors=excluded.authors,
    author_sort=excluded.author_sort,series=excluded.series,position=excluded.position,
    title_key=excluded.title_key,author_key=excluded.author_key,
    series_key=excluded.series_key,position_key=excluded.position_key,
    added=excluded.added,published=excluded.published,words=excluded.words,language=excluded.language,
    spine_rgb=excluded.spine_rgb,cover_rgb=excluded.cover_rgb,cover_digest=excluded.cover_digest,
    rtl=excluded.rtl,survey_version=excluded.survey_version,
    last_read=CASE WHEN version_id=excluded.version_id THEN last_read END,
    progress=CASE WHEN version_id=excluded.version_id THEN progress END,
    version_id=excluded.version_id
RETURNING n;
