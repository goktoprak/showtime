//! TMDB image URL builders, carried over from the previous plain-JS
//! frontend. TMDB returns bare path fragments; the size segment is chosen
//! per use site.

pub fn poster_url(path: &Option<String>) -> Option<String> {
    path.as_ref()
        .map(|p| format!("https://image.tmdb.org/t/p/w500{p}"))
}

pub fn backdrop_url(path: &Option<String>) -> Option<String> {
    path.as_ref()
        .map(|p| format!("https://image.tmdb.org/t/p/w780{p}"))
}
