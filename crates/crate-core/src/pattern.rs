//! Glob patterns for "Matching pattern…" and exclude lists: `*`, `?`,
//! case-insensitive. A pattern without `/` matches the file name; one with
//! `/` matches the whole path; one ending in `/` matches a folder anywhere.

/// Whether `path` matches any of the comma- or space-separated patterns.
pub fn matches_any(patterns: &str, path: &str) -> bool {
    split(patterns).any(|p| matches(p, path, false))
}

/// The patterns in a comma- or space-separated list.
pub fn split(patterns: &str) -> impl Iterator<Item = &str> {
    patterns
        .split([',', ' ', '\n'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
}

/// One pattern against a path (`is_dir`: the path is a folder).
pub fn matches(pattern: &str, path: &str, is_dir: bool) -> bool {
    let path = path.trim_start_matches("./").trim_end_matches('/');
    let name = path.rsplit('/').next().unwrap_or(path);
    if let Some(folder) = pattern.strip_suffix('/') {
        // `node_modules/`: that folder, wherever it is.
        return is_dir && glob(&folder.to_lowercase(), &name.to_lowercase())
            || path
                .split('/')
                .rev()
                .skip(1)
                .any(|part| glob(&folder.to_lowercase(), &part.to_lowercase()));
    }
    let target = if pattern.contains('/') { path } else { name };
    glob(&pattern.to_lowercase(), &target.to_lowercase())
}

fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    // Iterative wildcard match, backtracking to the last `*`.
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        assert!(matches_any("*.jpg", "photos/IMG_1.JPG"));
        assert!(matches_any("*.png, *.jpg", "a/b.png"));
        assert!(!matches_any("*.jpg", "a/b.jpeg"));
        assert!(matches_any("docs/*.pdf", "docs/x.pdf"));
        assert!(!matches_any("docs/*.pdf", "other/docs/x.pdf"));
        assert!(matches_any("img_??.*", "IMG_12.heic"));
        assert!(!matches_any("", "a"));
    }

    #[test]
    fn folder_patterns() {
        assert!(matches(".git/", "project/.git", true));
        assert!(matches(".git/", "project/.git/config", false));
        assert!(!matches(".git/", "project/.gitignore", false));
        assert!(matches("node_modules/", "a/node_modules/x/y.js", false));
        assert!(
            !matches(".git/", ".git", false),
            "a file called .git isn't a folder"
        );
    }
}
