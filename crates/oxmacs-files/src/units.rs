#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
    Utf32,
}

impl Encoding {
    pub fn named(name: Option<&str>) -> Self {
        match name {
            Some("utf-8") => Self::Utf8,
            Some("utf-32") => Self::Utf32,
            _ => Self::Utf16,
        }
    }

    fn units(self, character: char) -> usize {
        match self {
            Self::Utf8 => character.len_utf8(),
            Self::Utf16 => character.len_utf16(),
            Self::Utf32 => 1,
        }
    }

    pub fn to_units(self, line: &str, column: usize) -> usize {
        line.chars()
            .take(column)
            .map(|character| self.units(character))
            .sum()
    }

    pub fn to_column(self, line: &str, units: usize) -> usize {
        let mut counted = 0;
        let mut column = 0;
        for character in line.chars() {
            if counted >= units {
                break;
            }
            counted += self.units(character);
            column += 1;
        }
        column
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_count_in_the_server_s_units() {
        let line = "a😀é b";

        assert_eq!(Encoding::Utf16.to_units(line, 3), 4);
        assert_eq!(Encoding::Utf8.to_units(line, 3), 7);
        assert_eq!(Encoding::Utf32.to_units(line, 3), 3);
        assert_eq!(Encoding::Utf16.to_column(line, 4), 3);
        assert_eq!(Encoding::Utf8.to_column(line, 7), 3);
        assert_eq!(Encoding::Utf16.to_column(line, 99), 5);
        assert_eq!(Encoding::Utf16.to_column(line, 1), 1);
        assert_eq!(Encoding::Utf16.to_column(line, 2), 2);
        assert_eq!(Encoding::named(None), Encoding::Utf16);
        assert_eq!(Encoding::named(Some("utf-32")), Encoding::Utf32);
    }
}
