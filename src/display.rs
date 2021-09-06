use super::*;
impl std::fmt::Display for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldType::Concrete(s) => write!(f, "{}", s.to_token_stream()),
            FieldType::Structure(s) => write!(f, "{}", s),
        }
    }
}
impl std::fmt::Display for FieldSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for attr in self.attributes.iter() {
            writeln!(f, "{}", attr.to_token_stream())?;
        }
        write!(f, "{}: {}", self.ident, self.ty)?;
        if let Some(c) = &self.constraint {
            write!(f, " ({})", c.to_token_stream())?;
        }
        Ok(())
    }
}
impl std::fmt::Display for StructSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {{", &self.ident)?;
        for field in self.fields.iter() {
            write!(f, "{}, ", field)?;
        }
        write!(f, "}}")
    }
}
