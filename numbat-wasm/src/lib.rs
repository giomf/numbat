mod jquery_terminal_formatter;
mod utils;

use numbat::diagnostic::ErrorDiagnostic;
use numbat::module_importer::BuiltinModuleImporter;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;

use jquery_terminal_formatter::{jt_format, JqueryTerminalFormatter};

use numbat::markup::Formatter;
use numbat::pretty_print::PrettyPrint;
use numbat::resolver::CodeSource;
use numbat::{markup as m, NameResolutionError, NumbatError, Type};
use numbat::{Context, InterpreterResult, InterpreterSettings};

use crate::jquery_terminal_formatter::JqueryTerminalWriter;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
pub fn setup_panic_hook() {
    utils::set_panic_hook();
}

#[wasm_bindgen]
pub struct Numbat {
    ctx: Context,
}

#[wasm_bindgen]
impl Numbat {
    pub fn new() -> Self {
        let mut ctx = Context::new(BuiltinModuleImporter::default());
        let _ = ctx.interpret("use prelude", CodeSource::Internal).unwrap();
        Numbat { ctx }
    }

    pub fn interpret(&mut self, code: &str) -> String {
        let mut output = String::new();

        let registry = self.ctx.dimension_registry().clone();

        let fmt = JqueryTerminalFormatter {};

        let to_be_printed: Arc<Mutex<Vec<m::Markup>>> = Arc::new(Mutex::new(vec![]));
        let to_be_printed_c = to_be_printed.clone();
        let mut settings = InterpreterSettings {
            print_fn: Box::new(move |s: &m::Markup| {
                to_be_printed_c.lock().unwrap().push(s.clone());
            }),
        };

        match self
            .ctx
            .interpret_with_settings(&mut settings, &code, CodeSource::Text)
        {
            Ok((statements, result)) => {
                // Pretty print
                output.push_str("\n");
                for statement in &statements {
                    output.push_str(&fmt.format(&statement.pretty_print(), true));
                    output.push_str("\n");
                }
                output.push_str("\n");

                // print(…) and type(…) results
                let to_be_printed = to_be_printed.lock().unwrap();
                for content in to_be_printed.iter() {
                    output.push_str(&fmt.format(content, true));
                    output.push_str("\n");
                }

                match result {
                    InterpreterResult::Value(value) => {
                        if !to_be_printed.is_empty() {
                            output.push_str("\n");
                        }

                        // TODO: the following statement is copied from numbat-cli. Move this to the numbat crate
                        // to avoid duplication.
                        let type_ = statements.last().map_or(m::empty(), |s| {
                            if let numbat::Statement::Expression(e) = s {
                                let type_ = e.get_type();

                                if type_ == Type::scalar() {
                                    m::empty()
                                } else {
                                    m::dimmed("    [")
                                        + e.get_type().to_readable_type(&registry)
                                        + m::dimmed("]")
                                }
                            } else {
                                m::empty()
                            }
                        });

                        let markup = m::whitespace("    ")
                            + m::operator("=")
                            + m::space()
                            + value.pretty_print()
                            + type_;
                        output.push_str(&fmt.format(&markup, true));
                    }
                    InterpreterResult::Continue => {}
                    InterpreterResult::Exit(_) => {
                        output.push_str(&jt_format(Some("error"), "Error!".into()))
                    }
                }

                output
            }
            Err(NumbatError::ResolverError(e)) => self.print_diagnostic(&e),
            Err(NumbatError::NameResolutionError(
                e @ (NameResolutionError::IdentifierClash { .. }
                | NameResolutionError::ReservedIdentifier(_)),
            )) => self.print_diagnostic(&e),
            Err(NumbatError::TypeCheckError(e)) => self.print_diagnostic(&e),
            Err(NumbatError::RuntimeError(e)) => self.print_diagnostic(&e),
        }
    }

    pub fn get_completions_for(&self, input: &str) -> Vec<JsValue> {
        self.ctx
            .get_completions_for(input)
            .map(|s| s.trim().trim_end_matches('(').into())
            .collect()
    }

    fn print_diagnostic(&self, error: &dyn ErrorDiagnostic) -> String {
        use codespan_reporting::term::{self, Config};

        let mut writer = JqueryTerminalWriter::new();
        let config = Config::default();

        let resolver = self.ctx.resolver();

        term::emit(&mut writer, &config, &resolver.files, &error.diagnostic()).unwrap();

        writer.to_string()
    }
}
