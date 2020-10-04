use crate::data::{HttpMockRequest, RequestRequirements};
use difference::{Changeset, Difference};

pub(crate) mod comparators;
pub(crate) mod decoders;
pub(crate) mod generic;
pub(crate) mod sources;
pub(crate) mod targets;
mod util;

use basic_cookies::Cookie;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Display;

// *************************************************************************************************
// Diff and Change correspond to difference::Changeset and Difference structs. They are duplicated
// here only for the reason to make them serializable/deserializable using serde.
// *************************************************************************************************
#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub(crate) enum Diff {
    Same(String),
    Add(String),
    Rem(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DetailedDiffResult {
    pub differences: Vec<Diff>,
    pub distance: i32,
    pub tokenizer: Tokenizer,
}

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone, Copy)]
pub(crate) enum Tokenizer {
    Line,
    Word,
    Character,
}

pub(crate) fn diff_str(base: &str, edit: &str, tokenizer: Tokenizer) -> DetailedDiffResult {
    let splitter = match tokenizer {
        Tokenizer::Line => "\n",
        Tokenizer::Word => " ",
        Tokenizer::Character => "",
    };

    let changes = Changeset::new(base, edit, splitter);
    DetailedDiffResult {
        tokenizer,
        distance: changes.distance,
        differences: changes
            .diffs
            .iter()
            .map(|d| match d {
                Difference::Same(v) => Diff::Same(v.to_string()),
                Difference::Add(v) => Diff::Add(v.to_string()),
                Difference::Rem(v) => Diff::Rem(v.to_string()),
            })
            .collect(),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SimpleDiffResult {
    pub expected: String,
    pub actual: String,
    pub operation_name: String,
    pub best_match: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Mismatch {
    pub title: String,
    pub message: Option<String>,
    pub reason: Option<SimpleDiffResult>,
    pub detailed_diff: Option<DetailedDiffResult>,
    pub score: usize,
}

pub(crate) trait Matcher {
    fn matches(&self, req: &HttpMockRequest, mock: &RequestRequirements) -> bool;
    fn mismatches(&self, req: &HttpMockRequest, mock: &RequestRequirements) -> Vec<Mismatch>;
}

// *************************************************************************************************
// Helper functions
// *************************************************************************************************
pub(crate) fn parse_cookies(req: &HttpMockRequest) -> Result<BTreeMap<String, String>, String> {
    let parsing_result = req.headers.as_ref().map_or(None, |request_headers| {
        request_headers
            .iter()
            .find(|(k, _)| k.to_lowercase().eq("cookie"))
            .map(|(k, v)| Cookie::parse(v))
    });

    match parsing_result {
        None => Ok(BTreeMap::new()),
        Some(res) => match res {
            Err(e) => Err(e.to_string()),
            Ok(v) => Ok(v
                .into_iter()
                .map(|c| (c.get_name().to_lowercase(), c.get_value().to_owned()))
                .collect()),
        },
    }
}

pub(crate) fn distance_for(expected: &str, actual: &str) -> usize {
    let max_distance = (expected.len() + actual.len());
    if max_distance == 0 {
        return 0;
    }
    let distance = levenshtein::levenshtein(expected, actual);
    100 - ((max_distance - distance) / max_distance)
}

pub(crate) fn distance_for_vec(expected: &str, actual: &Vec<String>) -> usize {
    actual.into_iter().map(|e| distance_for(expected, e)).sum()
}

pub(crate) fn distance_for_opt<T, U>(expected: &Option<&T>, actual: &Option<&U>) -> usize
where
    T: Display,
    U: Display,
{
    let expected = expected.map_or(String::new(), |x| x.to_string());
    let actual = actual.map_or(String::new(), |x| x.to_string());
    distance_for(&expected, &actual)
}

fn diff_str_new(s1: &str, s2: &str) -> usize {
    levenshtein::levenshtein(s1, s2)
}
