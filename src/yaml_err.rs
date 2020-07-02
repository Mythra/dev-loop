//! Utility functions to help with reporting YAML Errors to
//! a user, showing things like where specifically the yaml
//! failed to parse where possible.

use crate::{strsim::calculate_did_you_mean_possibilities_str, terminal::TERM};
use annotate_snippets::{
	display_list::{DisplayList, FormatOptions},
	snippet::{AnnotationType, Slice, Snippet, SourceAnnotation},
};
use color_eyre::{
	eyre::{eyre, WrapErr},
	section::help::Help,
	Report,
};
use serde_yaml::Error as YamlError;

/// Location doesn't implement copy/clone, so we create a hacky struct to clone it into.
struct LocationCopy {
	/// The column this error occurs at.
	pub column: usize,
	/// The index of location
	pub index: usize,
	/// The line this error occurs at.
	pub line: usize,
}

/// Convert a `serde_yaml::Location`, into a `LocationCopy` struct.
fn loc_to_loc_copy(loc: Option<serde_yaml::Location>) -> Option<LocationCopy> {
	if let Some(loc_ref) = loc {
		Some(LocationCopy {
			column: loc_ref.column(),
			index: loc_ref.index(),
			line: loc_ref.line(),
		})
	} else {
		None
	}
}

/// Add "did you mean" text to unknown variant error message for YAML.
///
/// `err_msg` - the YAML error message.
fn did_you_mean_variant(err_msg: &str) -> Option<Vec<String>> {
	let them_split = err_msg.splitn(2, ": ").collect::<Vec<&str>>();
	if them_split.len() != 2 {
		return None;
	}
	let err_msg_without_field = them_split[1];
	if !err_msg_without_field.starts_with("unknown variant `") {
		return None;
	}

	let mut variant_errs = Vec::new();

	let mut parsing = false;
	let mut buff = String::new();
	for the_char in err_msg_without_field.chars() {
		if parsing {
			if the_char == '`' {
				variant_errs.push(buff);
				buff = String::new();
				parsing = false;
			} else {
				buff.push(the_char);
			}
		} else if the_char == '`' {
			parsing = true;
		}
	}

	if variant_errs.len() < 2 {
		None
	} else {
		let (what_was_typed, possibilities) = variant_errs.split_first().unwrap();
		Some(calculate_did_you_mean_possibilities_str(
			what_was_typed,
			possibilities,
			3,
		))
	}
}

/// Add contextulization to a YAML Error.
///
/// `result` - the result to contextualize.
/// `src_filepath` - the file path of the source error.
/// `src_data` - the file contents.
///
/// # Errors
///
/// - If the first parameter error'd.
pub fn contextualize_yaml_err<T>(
	result: Result<T, YamlError>,
	src_filepath: &str,
	src_data: &str,
) -> Result<T, Report> {
	match result {
		Ok(success) => Ok(success),
		Err(yaml_err) => {
			let loc_clone = loc_to_loc_copy(yaml_err.location());
			let mut new_err: Result<T, Report> = Err(eyre!("Failed to parse as yaml"));

			let formatted_err_str = format!("{}", yaml_err);
			let formatted_err_str_clone = formatted_err_str.clone();

			if let Some(source_loc) = loc_clone {
				new_err = new_err.with_section(move || {
					let snippet = Snippet {
						title: None,
						footer: vec![],
						slices: vec![Slice {
							source: &src_data,
							line_start: 1,
							origin: Some(src_filepath),
							annotations: vec![SourceAnnotation {
								range: (source_loc.index, source_loc.index + 1),
								label: &formatted_err_str_clone,
								annotation_type: AnnotationType::Error,
							}],
							fold: true,
						}],
						opt: FormatOptions {
							// Errors get put on STDOUT.
							color: TERM.should_color_stdout(),
							anonymized_line_numbers: false,
							margin: None,
						},
					};

					DisplayList::from(snippet).to_string()
				});
			} else {
				new_err = Err(yaml_err).wrap_err("Failed to parse as yaml");
				new_err = new_err.note("A specific line could not be derived for this error.");
			}

			if let Some(did_you_mean_strings) = did_you_mean_variant(&formatted_err_str) {
				for did_you_mean_text in did_you_mean_strings {
					new_err = new_err.suggestion(did_you_mean_text);
				}
			}

			new_err
		}
	}
}
