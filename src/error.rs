pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct Dyno {
    pub source: Box<dyn std::error::Error>,
    pub file: &'static str,
    pub line: u32,
}

#[macro_export]
macro_rules! wrap {
    ($source:expr) => {
        $crate::error::Dyno {
            source: $source,
            file: file!(),
            line: line!(),
        }
    };
}

impl std::fmt::Display for Dyno {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_error_stack())
    }
}

impl std::error::Error for Dyno {}

impl std::fmt::Debug for Dyno {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_error_stack())
    }
}

impl Dyno {
    fn format_error_stack(&self) -> String {
        let mut result = format!("Error in file and line -> {}:{}\n", self.file, self.line);

        let mut current_error: &dyn std::error::Error = &*self.source;
        while let Some(source) = current_error.downcast_ref::<Dyno>() {
            result.push_str(&format!(
                "\nCaused by:\n  Error in file and line -> {}:{}\n      source: {}",
                source.file, source.line, source.source
            ));
            current_error = &*source.source;
        }
        result
    }
}
