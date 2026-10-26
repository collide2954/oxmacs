use crate::error::Error;
use crate::read::{Datum, Syntax};

#[derive(Default)]
pub struct Parameters<'s> {
    pub required: Vec<&'s Syntax>,
    pub optional: Vec<(&'s Syntax, Option<&'s Syntax>)>,
    pub rest: Option<&'s Syntax>,
    pub keywords: Vec<(&'s Syntax, &'s Syntax)>,
}

fn is_name(syntax: &Syntax) -> bool {
    matches!(&syntax.datum, Datum::Symbol(name) if name != "&")
}

pub fn parse_parameters(list: &[Syntax]) -> Result<Parameters<'_>, Error> {
    let mut found = Parameters::default();
    let mut items = list.iter().peekable();
    while let Some(item) = items.next() {
        let at = |message: &str| Error::at(item.span, message);
        match &item.datum {
            Datum::Symbol(name) if name == "&" => {
                let Some(rest) = items.next().filter(|rest| is_name(rest)) else {
                    return Err(at("& takes one name for the rest of the arguments"));
                };
                found.rest = Some(rest);
                if let Some(next) = items.peek()
                    && !matches!(next.datum, Datum::Map(_))
                {
                    return Err(Error::at(
                        next.span,
                        "& takes one name for the rest of the arguments",
                    ));
                }
            }
            Datum::Symbol(_) | Datum::Vector(_) | Datum::Record(..) => {
                if !found.optional.is_empty() || found.rest.is_some() {
                    return Err(at("Required parameters come before optional ones"));
                }
                found.required.push(item);
            }
            Datum::List(parts) => match parts.as_slice() {
                [name] if is_name(name) => found.optional.push((name, None)),
                [name, default] if is_name(name) => found.optional.push((name, Some(default))),
                _ => return Err(at("Parameters must be names")),
            },
            Datum::Map(pairs) => {
                if items.peek().is_some() {
                    return Err(at("Keyword parameters come last"));
                }
                for pair in pairs.chunks(2) {
                    let [name, default] = pair else {
                        return Err(at("Parameters must be names"));
                    };
                    if !is_name(name) {
                        return Err(at("Parameters must be names"));
                    }
                    found.keywords.push((name, default));
                }
            }
            _ => return Err(at("Parameters must be names")),
        }
    }
    Ok(found)
}
