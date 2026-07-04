// Small fetch wrapper used across all pages.
async function api(path, options = {}) {
    const res = await fetch(`/api${path}`, {
        headers: { "Content-Type": "application/json" },
        ...options,
    });
    if (!res.ok) {
        let msg = `Request failed (${res.status})`;
        try {
            const body = await res.json();
            if (body.error) msg = body.error;
        } catch (_) {}
        throw new Error(msg);
    }
    if (res.status === 204) return null;
    return res.json();
}

function posterUrl(path) {
    if (!path) return null;
    return `https://image.tmdb.org/t/p/w500${path}`;
}

function backdropUrl(path) {
    if (!path) return null;
    return `https://image.tmdb.org/t/p/w780${path}`;
}
