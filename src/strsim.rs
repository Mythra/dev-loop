//! A utility function for determining if a string is similar.

use color_eyre::{Report, Section};
use std::collections::HashMap;

/// Returns the final index for a value in a single vector that represents a fixed
/// 2d grid.
fn flat_index(i: usize, j: usize, width: usize) -> usize {
	j * width + i
}

/// Determine the Damerau Levenshtein distance between two generic arrays.
pub fn generic_string_differences<Elem>(a_elems: &[Elem], b_elems: &[Elem]) -> usize
where
	Elem: Eq + std::hash::Hash + Clone,
{
	let a_len = a_elems.len();
	let b_len = b_elems.len();

	if a_len == 0 {
		return b_len;
	}
	if b_len == 0 {
		return a_len;
	}

	let width = a_len + 2;
	let mut distances = vec![0; (a_len + 2) * (b_len + 2)];
	let max_distance = a_len + b_len;
	distances[0] = max_distance;

	for i in 0..=a_len {
		distances[flat_index(i + 1, 0, width)] = max_distance;
		distances[flat_index(i + 1, 1, width)] = i;
	}

	for j in 0..=b_len {
		distances[flat_index(0, j + 1, width)] = max_distance;
		distances[flat_index(1, j + 1, width)] = j;
	}

	let mut elems: HashMap<Elem, usize> = HashMap::with_capacity(64);

	for i in 1..=a_len {
		let mut db = 0;

		for j in 1..=b_len {
			let k = match elems.get(&b_elems[j - 1]) {
				Some(&value) => value,
				None => 0,
			};

			let insertion_cost = distances[flat_index(i, j + 1, width)] + 1;
			let deletion_cost = distances[flat_index(i + 1, j, width)] + 1;
			let transposition_cost =
				distances[flat_index(k, db, width)] + (i - k - 1) + 1 + (j - db - 1);

			let mut substitution_cost = distances[flat_index(i, j, width)] + 1;
			if a_elems[i - 1] == b_elems[j - 1] {
				db = j;
				substitution_cost -= 1;
			}

			distances[flat_index(i + 1, j + 1, width)] = std::cmp::min(
				substitution_cost,
				std::cmp::min(
					insertion_cost,
					std::cmp::min(deletion_cost, transposition_cost),
				),
			);
		}

		elems.insert(a_elems[i - 1].clone(), i);
	}

	distances[flat_index(a_len + 1, b_len + 1, width)]
}

/// Determine the "distance" between two strings.
/// Uses a Damerau Levenshtein distance.
#[must_use]
pub fn string_differences(a: &str, b: &str) -> usize {
	let (x, y): (Vec<_>, Vec<_>) = (a.chars().collect(), b.chars().collect());
	generic_string_differences(x.as_slice(), y.as_slice())
}

/// Calculatea a series of possibilities for potentially typos.
///
/// `potentially_typod_thing`: the thing that was potentially typo'd.
/// `typo_possibilities`: the typo possibilities.
/// `distance`: the distance to calculate.
#[must_use]
pub fn calculate_did_you_mean_possibilities(
	potentially_typod_thing: &str,
	typo_possibilities: &[&str],
	distance: usize,
) -> Vec<String> {
	let mut suggestions = Vec::new();

	for typo_possibility in typo_possibilities {
		if string_differences(potentially_typod_thing, typo_possibility) <= distance {
			suggestions.push(format!(
				"Instead of: \"{}\", Did you mean: \"{}\"",
				potentially_typod_thing, typo_possibility
			));
		}
	}

	suggestions
}

/// Calculatea a series of possibilities for potentially typos.
///
/// `potentially_typod_thing`: the thing that was potentially typo'd.
/// `typo_possibilities`: the typo possibilities.
/// `distance`: the distance to calculate.
#[must_use]
pub fn calculate_did_you_mean_possibilities_str(
	potentially_typod_thing: &str,
	typo_possibilities: &[String],
	distance: usize,
) -> Vec<String> {
	let mut suggestions = Vec::new();

	for typo_possibility in typo_possibilities {
		if string_differences(potentially_typod_thing, typo_possibility) <= distance {
			suggestions.push(format!(
				"Instead of: \"{}\", Did you mean: \"{}\"",
				potentially_typod_thing, typo_possibility
			));
		}
	}

	suggestions
}

/// Takes an error, and adds `did you mean: "blah"` notes when possible.
///
/// `result`: the result to add text too.
/// `potentially_typod_thing`: the thing to check against the list of possiblities.
/// `typo_possiblities`: the list of possibilities to match against.
/// `distance`: the distance to calculate.
///
/// # Errors
///
/// When there is an error in the result parameter.
///
/// # Returns
///
/// If we have added a suggestion.
pub fn add_did_you_mean_text<T>(
	mut result: Result<T, Report>,
	potentially_typod_thing: &str,
	typo_possibilities: &[&str],
	distance: usize,
	default_msg: Option<&'static str>,
) -> Result<T, Report> {
	let mut has_suggested = false;

	for typo_possibility in typo_possibilities {
		if string_differences(potentially_typod_thing, typo_possibility) <= distance {
			result = result.suggestion(format!(
				"Instead of: \"{}\", Did you mean: \"{}\"",
				potentially_typod_thing, typo_possibility
			));
			has_suggested = true;
		}
	}

	if let Some(def_msg) = default_msg {
		if !has_suggested {
			result = result.note(def_msg);
		}
	}

	result
}
