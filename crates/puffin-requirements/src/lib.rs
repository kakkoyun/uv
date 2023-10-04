use std::borrow::Cow;
use std::ops::Deref;
use std::str::FromStr;

use anyhow::Result;
use memchr::{memchr2, memchr_iter};
use pep508_rs::{Pep508Error, Requirement};

#[derive(Debug)]
pub struct Requirements(Vec<Requirement>);

impl FromStr for Requirements {
    type Err = Pep508Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            RequirementsIterator::new(s)
                .map(|requirement| Requirement::from_str(requirement.as_str()))
                .collect::<Result<Vec<Requirement>, Pep508Error>>()?,
        ))
    }
}

impl Deref for Requirements {
    type Target = [Requirement];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
struct RequirementsIterator<'a> {
    text: &'a str,
    index: usize,
}

impl<'a> RequirementsIterator<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, index: 0 }
    }
}

#[derive(Debug)]
struct RequirementLine<'a> {
    /// The line as included in the `requirements.txt`, including comments and `--hash` extras.
    line: Cow<'a, str>,
    /// The line, with comments and `--hash` extras stripped.
    len: usize,
}

impl<'a> RequirementLine<'a> {
    /// Create a new `RequirementLine` from a line of text.
    fn from_line(line: Cow<'a, str>) -> Self {
        Self {
            len: Self::strip_trivia(&line),
            line,
        }
    }

    /// Return a parseable requirement line.
    fn as_str(&self) -> &str {
        &self.line[..self.len]
    }

    /// Strip trivia (comments and `--hash` extras) from a requirement, returning the length of the
    /// requirement itself.
    fn strip_trivia(requirement: &str) -> usize {
        let mut len = requirement.len();

        // Strip comments.
        for position in memchr_iter(b'#', requirement[..len].as_bytes()) {
            // The comment _must_ be preceded by whitespace.
            if requirement[..len + position]
                .chars()
                .rev()
                .next()
                .is_some_and(char::is_whitespace)
            {
                len = position;
                break;
            }
        }

        // Strip `--hash` extras.
        if let Some(index) = requirement[..len].find("--hash") {
            len = index;
        }

        len
    }
}

impl<'a> Iterator for RequirementsIterator<'a> {
    type Item = RequirementLine<'a>;

    #[inline]
    fn next(&mut self) -> Option<RequirementLine<'a>> {
        if self.index == self.text.len() - 1 {
            return None;
        }

        // Find the next line break.
        let Some((start, length)) = find_newline(&self.text[self.index..]) else {
            // Parse the rest of the text.
            let line = &self.text[self.index..];
            self.index = self.text.len() - 1;

            // Skip fully-commented lines.
            if line.trim_start().starts_with('#') {
                return None;
            }

            // Skip empty lines.
            if line.trim().is_empty() {
                return None;
            }

            return Some(RequirementLine::from_line(Cow::Borrowed(line)));
        };

        // Skip fully-commented lines.
        if self.text[self.index..].trim_start().starts_with('#') {
            self.index += start + length;
            return self.next();
        }

        // Skip empty lines.
        if self.text[self.index..self.index + start].trim().is_empty() {
            self.index += start + length;
            return self.next();
        }

        // If the newline is preceded by a continuation (\\), keep going.
        if self.text[..self.index + start]
            .chars()
            .rev()
            .next()
            .is_some_and(|c| c == '\\')
        {
            // Add the line contents, preceding the continuation.
            let mut line = self.text[self.index..self.index + start - 1].to_owned();
            self.index += start + length;

            // Eat lines until we see a non-continuation.
            while let Some((start, length)) = find_newline(&self.text[self.index..]) {
                if self.text[..self.index + start]
                    .chars()
                    .rev()
                    .next()
                    .is_some_and(|c| c == '\\')
                {
                    // Add the line contents, preceding the continuation.
                    line.push_str(&self.text[self.index..self.index + start - 1]);
                    self.index += start + length;
                } else {
                    // Add the line contents, excluding the continuation.
                    line.push_str(&self.text[self.index..self.index + start]);
                    self.index += start + length;
                    break;
                }
            }

            Some(RequirementLine::from_line(Cow::Owned(line)))
        } else {
            let line = &self.text[self.index..self.index + start];
            self.index += start + length;
            Some(RequirementLine::from_line(Cow::Borrowed(line)))
        }
    }
}

/// Return the start and end position of the first newline character in the given text.
#[inline]
fn find_newline(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let position = memchr2(b'\n', b'\r', bytes)?;

    // SAFETY: memchr guarantees to return valid positions
    #[allow(unsafe_code)]
    let newline_character = unsafe { *bytes.get_unchecked(position) };

    Some(match newline_character {
        // Explicit branch for `\n` as this is the most likely path
        b'\n' => (position, 1),
        // '\r\n'
        b'\r' if bytes.get(position.saturating_add(1)) == Some(&b'\n') => (position, 2),
        // '\r'
        _ => (position, 1),
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use insta::assert_debug_snapshot;

    use crate::Requirements;
    use anyhow::Result;

    #[test]
    fn simple() -> Result<()> {
        assert_debug_snapshot!(Requirements::from_str(r#"flask==2.0"#)?);
        Ok(())
    }

    #[test]
    fn pip_compile() -> Result<()> {
        assert_debug_snapshot!(Requirements::from_str(
            r#"
#
# This file is autogenerated by pip-compile with Python 3.7
# by the following command:
#
#    pip-compile --generate-hashes --output-file=requirements.txt --resolver=backtracking pyproject.toml
#
attrs==23.1.0 \
    --hash=sha256:1f28b4522cdc2fb4256ac1a020c78acf9cba2c6b461ccd2c126f3aa8e8335d04 \
    --hash=sha256:6279836d581513a26f1bf235f9acd333bc9115683f14f7e8fae46c98fc50e015
    # via
    #   cattrs
    #   lsprotocol
cattrs==23.1.2 \
    --hash=sha256:b2bb14311ac17bed0d58785e5a60f022e5431aca3932e3fc5cc8ed8639de50a4 \
    --hash=sha256:db1c821b8c537382b2c7c66678c3790091ca0275ac486c76f3c8f3920e83c657
    # via lsprotocol
exceptiongroup==1.1.3 \
    --hash=sha256:097acd85d473d75af5bb98e41b61ff7fe35efe6675e4f9370ec6ec5126d160e9 \
    --hash=sha256:343280667a4585d195ca1cf9cef84a4e178c4b6cf2274caef9859782b567d5e3
    # via cattrs
importlib-metadata==6.7.0 \
    --hash=sha256:1aaf550d4f73e5d6783e7acb77aec43d49da8017410afae93822cc9cca98c4d4 \
    --hash=sha256:cb52082e659e97afc5dac71e79de97d8681de3aa07ff18578330904a9d18e5b5
    # via
    #   attrs
    #   typeguard
lsprotocol==2023.0.0b1 \
    --hash=sha256:ade2cd0fa0ede7965698cb59cd05d3adbd19178fd73e83f72ef57a032fbb9d62 \
    --hash=sha256:f7a2d4655cbd5639f373ddd1789807450c543341fa0a32b064ad30dbb9f510d4
    # via
    #   pygls
    #   ruff-lsp (pyproject.toml)
packaging==23.2 \
    --hash=sha256:048fb0e9405036518eaaf48a55953c750c11e1a1b68e0dd1a9d62ed0c092cfc5 \
    --hash=sha256:8c491190033a9af7e1d931d0b5dacc2ef47509b34dd0de67ed209b5203fc88c7
    # via ruff-lsp (pyproject.toml)
pygls==1.1.0 \
    --hash=sha256:70acb6fe0df1c8a17b7ce08daa0afdb4aedc6913a6a6696003e1434fda80a06e \
    --hash=sha256:eb19b818039d3d705ec8adbcdf5809a93af925f30cd7a3f3b7573479079ba00e
    # via ruff-lsp (pyproject.toml)
ruff==0.0.292 \
    --hash=sha256:02f29db018c9d474270c704e6c6b13b18ed0ecac82761e4fcf0faa3728430c96 \
    --hash=sha256:1093449e37dd1e9b813798f6ad70932b57cf614e5c2b5c51005bf67d55db33ac \
    --hash=sha256:69654e564342f507edfa09ee6897883ca76e331d4bbc3676d8a8403838e9fade \
    --hash=sha256:6bdfabd4334684a4418b99b3118793f2c13bb67bf1540a769d7816410402a205 \
    --hash=sha256:6c3c91859a9b845c33778f11902e7b26440d64b9d5110edd4e4fa1726c41e0a4 \
    --hash=sha256:7f67a69c8f12fbc8daf6ae6d36705037bde315abf8b82b6e1f4c9e74eb750f68 \
    --hash=sha256:87616771e72820800b8faea82edd858324b29bb99a920d6aa3d3949dd3f88fb0 \
    --hash=sha256:8e087b24d0d849c5c81516ec740bf4fd48bf363cfb104545464e0fca749b6af9 \
    --hash=sha256:9889bac18a0c07018aac75ef6c1e6511d8411724d67cb879103b01758e110a81 \
    --hash=sha256:aa7c77c53bfcd75dbcd4d1f42d6cabf2485d2e1ee0678da850f08e1ab13081a8 \
    --hash=sha256:ac153eee6dd4444501c4bb92bff866491d4bfb01ce26dd2fff7ca472c8df9ad0 \
    --hash=sha256:b76deb3bdbea2ef97db286cf953488745dd6424c122d275f05836c53f62d4016 \
    --hash=sha256:be8eb50eaf8648070b8e58ece8e69c9322d34afe367eec4210fdee9a555e4ca7 \
    --hash=sha256:e854b05408f7a8033a027e4b1c7f9889563dd2aca545d13d06711e5c39c3d003 \
    --hash=sha256:f160b5ec26be32362d0774964e218f3fcf0a7da299f7e220ef45ae9e3e67101a \
    --hash=sha256:f27282bedfd04d4c3492e5c3398360c9d86a295be00eccc63914438b4ac8a83c \
    --hash=sha256:f4476f1243af2d8c29da5f235c13dca52177117935e1f9393f9d90f9833f69e4
    # via ruff-lsp (pyproject.toml)
typeguard==3.0.2 \
    --hash=sha256:bbe993854385284ab42fd5bd3bee6f6556577ce8b50696d6cb956d704f286c8e \
    --hash=sha256:fee5297fdb28f8e9efcb8142b5ee219e02375509cd77ea9d270b5af826358d5a
    # via pygls
typing-extensions==4.7.1 \
    --hash=sha256:440d5dd3af93b060174bf433bccd69b0babc3b15b1a8dca43789fd7f61514b36 \
    --hash=sha256:b75ddc264f0ba5615db7ba217daeb99701ad295353c45f9e95963337ceeeffb2
    # via
    #   cattrs
    #   importlib-metadata
    #   ruff-lsp (pyproject.toml)
    #   typeguard
zipp==3.15.0 \
    --hash=sha256:112929ad649da941c23de50f356a2b5570c954b65150642bccdd66bf194d224b \
    --hash=sha256:48904fc76a60e542af151aded95726c1a5c34ed43ab4134b597665c86d7ad556
    # via importlib-metadata
"#
        )?);
        Ok(())
    }
}
