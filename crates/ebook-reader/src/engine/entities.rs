//! Character references. XML's five, numeric references and the named set that
//! XHTML 1.1 defines, which books use without the parser reading any DTD.

/// Append `raw` to `out` with character references replaced.
/// An unknown or malformed reference stays as written.
pub fn unescape_into(raw: &str, out: &mut String) {
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp + 1..];
        let end = tail.bytes().take(34).position(|b| b == b';');
        match end.and_then(|e| resolve(&tail[..e]).map(|c| (e, c))) {
            Some((e, c)) => {
                out.push(c);
                rest = &tail[e + 1..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
}

pub fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    unescape_into(raw, &mut out);
    out
}

fn resolve(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) if !hex.is_empty() && hex.len() <= 8 => u32::from_str_radix(hex, 16).ok()?,
            Some(_) => return None,
            None if !num.is_empty() && num.len() <= 10 && num.bytes().all(|b| b.is_ascii_digit()) => {
                num.parse().ok()?
            }
            None => return None,
        };
        return Some(char::from_u32(code).filter(|c| *c != '\0').unwrap_or('\u{fffd}'));
    }
    if let Some(i) = LATIN1.iter().position(|n| *n == name) {
        return char::from_u32(160 + i as u32);
    }
    OTHER.iter().find(|(n, _)| *n == name).and_then(|(_, code)| char::from_u32(*code))
}

/// U+00A0 to U+00FF in order.
const LATIN1: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf",
    "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro",
    "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute",
    "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute",
    "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute",
    "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde",
    "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute",
    "ucirc", "uuml", "yacute", "thorn", "yuml",
];

const OTHER: &[(&str, u32)] = &[
    ("amp", 38), ("lt", 60), ("gt", 62), ("quot", 34), ("apos", 39),
    ("OElig", 338), ("oelig", 339), ("Scaron", 352), ("scaron", 353), ("Yuml", 376),
    ("fnof", 402), ("circ", 710), ("tilde", 732),
    ("Alpha", 913), ("Beta", 914), ("Gamma", 915), ("Delta", 916), ("Epsilon", 917),
    ("Zeta", 918), ("Eta", 919), ("Theta", 920), ("Iota", 921), ("Kappa", 922),
    ("Lambda", 923), ("Mu", 924), ("Nu", 925), ("Xi", 926), ("Omicron", 927), ("Pi", 928),
    ("Rho", 929), ("Sigma", 931), ("Tau", 932), ("Upsilon", 933), ("Phi", 934), ("Chi", 935),
    ("Psi", 936), ("Omega", 937),
    ("alpha", 945), ("beta", 946), ("gamma", 947), ("delta", 948), ("epsilon", 949),
    ("zeta", 950), ("eta", 951), ("theta", 952), ("iota", 953), ("kappa", 954),
    ("lambda", 955), ("mu", 956), ("nu", 957), ("xi", 958), ("omicron", 959), ("pi", 960),
    ("rho", 961), ("sigmaf", 962), ("sigma", 963), ("tau", 964), ("upsilon", 965),
    ("phi", 966), ("chi", 967), ("psi", 968), ("omega", 969), ("thetasym", 977),
    ("upsih", 978), ("piv", 982),
    ("ensp", 8194), ("emsp", 8195), ("thinsp", 8201), ("zwnj", 8204), ("zwj", 8205),
    ("lrm", 8206), ("rlm", 8207), ("ndash", 8211), ("mdash", 8212), ("lsquo", 8216),
    ("rsquo", 8217), ("sbquo", 8218), ("ldquo", 8220), ("rdquo", 8221), ("bdquo", 8222),
    ("dagger", 8224), ("Dagger", 8225), ("bull", 8226), ("hellip", 8230), ("permil", 8240),
    ("prime", 8242), ("Prime", 8243), ("lsaquo", 8249), ("rsaquo", 8250), ("oline", 8254),
    ("frasl", 8260), ("euro", 8364), ("image", 8465), ("weierp", 8472), ("real", 8476),
    ("trade", 8482), ("alefsym", 8501), ("larr", 8592), ("uarr", 8593), ("rarr", 8594),
    ("darr", 8595), ("harr", 8596), ("crarr", 8629), ("lArr", 8656), ("uArr", 8657),
    ("rArr", 8658), ("dArr", 8659), ("hArr", 8660), ("forall", 8704), ("part", 8706),
    ("exist", 8707), ("empty", 8709), ("nabla", 8711), ("isin", 8712), ("notin", 8713),
    ("ni", 8715), ("prod", 8719), ("sum", 8721), ("minus", 8722), ("lowast", 8727),
    ("radic", 8730), ("prop", 8733), ("infin", 8734), ("ang", 8736), ("and", 8743),
    ("or", 8744), ("cap", 8745), ("cup", 8746), ("int", 8747), ("there4", 8756),
    ("sim", 8764), ("cong", 8773), ("asymp", 8776), ("ne", 8800), ("equiv", 8801),
    ("le", 8804), ("ge", 8805), ("sub", 8834), ("sup", 8835), ("nsub", 8836), ("sube", 8838),
    ("supe", 8839), ("oplus", 8853), ("otimes", 8855), ("perp", 8869), ("sdot", 8901),
    ("lceil", 8968), ("rceil", 8969), ("lfloor", 8970), ("rfloor", 8971), ("lang", 9001),
    ("rang", 9002), ("loz", 9674), ("spades", 9824), ("clubs", 9827), ("hearts", 9829),
    ("diams", 9830),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_known_references_and_keeps_the_rest() {
        assert_eq!(unescape("a &amp; b &lt;c&gt;"), "a & b <c>");
        assert_eq!(unescape("&ldquo;Hi&rdquo;&nbsp;&mdash;&hellip;"), "\u{201c}Hi\u{201d}\u{a0}\u{2014}\u{2026}");
        assert_eq!(unescape("&#233;&#xE9;&#x1F600;"), "\u{e9}\u{e9}\u{1f600}");
        assert_eq!(unescape("&eacute;&yuml;&Agrave;"), "\u{e9}\u{ff}\u{c0}");
        assert_eq!(unescape("AT&T &bogus; & &#xZZ; &"), "AT&T &bogus; & &#xZZ; &");
        assert_eq!(unescape("&#0;&#xD800;&#99999999;"), "\u{fffd}\u{fffd}\u{fffd}");
    }
}
