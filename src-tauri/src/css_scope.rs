//! Syntax-aware CSS validator for theme `components` and `motion` strings.
//!
//! The engine injects `components`/`motion` verbatim into a `<style>` element
//! on the Harness page, so these strings must be structurally proven safe
//! *before* they ever reach the WebView. This module parses the CSS with the
//! Servo `cssparser` + `selectors` crates (a real parser/AST, not a regex) and
//! fails closed on anything it cannot prove to be confined to `#root`.
//!
//! Rules enforced:
//!   * Every selector must begin with the `#root` id and may only descend
//!     (child / descendant / pseudo-element combinators). Sibling/column
//!     combinators are rejected because they can escape `#root`.
//!   * `:is()`, `:where()`, `:not()` and `:nth-child(... of S)` /
//!     `:nth-last-child(... of S)` are recursed with the same scope rules.
//!   * `:has()` is rejected outright (no relative-selector containment).
//!   * Only `@media`, `@supports` and `@keyframes` at-rules are allowed;
//!     `@media`/`@supports` bodies are validated recursively and every other
//!     at-rule (`@import`, `@font-face`, …) is rejected.
//!   * `@keyframes` names must use the `hd-` prefix; keyframe selectors
//!     (`from`, `to`, percentages) are treated as keyframe grammar, not DOM
//!     selectors.
//!   * Any URL-bearing token (`url(...)`, unquoted url, bad-url) is rejected
//!     everywhere, preventing remote/external resource loads.

use std::fmt;

use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser as CssParser, ParserInput,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, StyleSheetParser, ToCss, Token,
};
use precomputed_hash::PrecomputedHash;
use selectors::parser::{
    Combinator, Component, NonTSPseudoClass, ParseRelative, Parser as SelectorParser,
    PseudoElement, Selector, SelectorImpl, SelectorList, SelectorParseErrorKind,
};

/// An error produced while validating theme CSS. Carries a user-facing message.
#[derive(Clone, Debug)]
pub struct CssValidationError(String);

impl CssValidationError {
    fn msg<S: Into<String>>(s: S) -> Self {
        Self(s.into())
    }
}

impl fmt::Display for CssValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CssValidationError {}

impl<'i> From<SelectorParseErrorKind<'i>> for CssValidationError {
    fn from(kind: SelectorParseErrorKind<'i>) -> Self {
        Self(format!("selector parse error: {kind:?}"))
    }
}

// ---------------------------------------------------------------------------
// Selector implementation types (minimal, only used to obtain the AST)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
struct HdAtom(String);

impl HdAtom {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl ToCss for HdAtom {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        cssparser::serialize_identifier(&self.0, dest)
    }
}

impl<'a> From<&'a str> for HdAtom {
    fn from(s: &'a str) -> Self {
        Self(s.into())
    }
}

impl PrecomputedHash for HdAtom {
    fn precomputed_hash(&self) -> u32 {
        0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HdPseudoClass(String);

impl ToCss for HdPseudoClass {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        cssparser::serialize_identifier(&self.0, dest)
    }
}

impl NonTSPseudoClass for HdPseudoClass {
    type Impl = HdSelectorImpl;

    fn is_active_or_hover(&self) -> bool {
        self.0.eq_ignore_ascii_case("active") || self.0.eq_ignore_ascii_case("hover")
    }

    fn is_user_action_state(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HdPseudoElement(String);

impl ToCss for HdPseudoElement {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        cssparser::serialize_identifier(&self.0, dest)
    }
}

impl PseudoElement for HdPseudoElement {
    type Impl = HdSelectorImpl;
}

#[derive(Clone, Debug, PartialEq)]
struct HdSelectorImpl;

impl SelectorImpl for HdSelectorImpl {
    type ExtraMatchingData<'a> = std::marker::PhantomData<&'a ()>;
    type AttrValue = HdAtom;
    type Identifier = HdAtom;
    type LocalName = HdAtom;
    type NamespaceUrl = HdAtom;
    type NamespacePrefix = HdAtom;
    type BorrowedNamespaceUrl = HdAtom;
    type BorrowedLocalName = HdAtom;
    type NonTSPseudoClass = HdPseudoClass;
    type PseudoElement = HdPseudoElement;
}

/// Selector parser config: parse `:is`/`:where` and `:nth-child(... of S)`
/// (we validate them recursively), but reject `:has` and nesting.
struct HdSelectorParser;

impl<'i> SelectorParser<'i> for HdSelectorParser {
    type Impl = HdSelectorImpl;
    type Error = CssValidationError;

    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn parse_nth_child_of(&self) -> bool {
        true
    }

    fn parse_non_ts_pseudo_class(
        &self,
        _location: cssparser::SourceLocation,
        name: CowRcStr<'i>,
    ) -> Result<HdPseudoClass, ParseError<'i, CssValidationError>> {
        Ok(HdPseudoClass(name.to_string()))
    }

    fn parse_non_ts_functional_pseudo_class<'t>(
        &self,
        name: CowRcStr<'i>,
        parser: &mut CssParser<'i, 't>,
        _after_part: bool,
    ) -> Result<HdPseudoClass, ParseError<'i, CssValidationError>> {
        if name.eq_ignore_ascii_case("has") {
            return Err(parser.new_custom_error(CssValidationError::msg(
                ":has() is not supported in theme CSS",
            )));
        }
        Err(parser.new_custom_error(CssValidationError::msg(format!(
            "unsupported functional pseudo-class :{name}()"
        ))))
    }

    fn parse_pseudo_element(
        &self,
        _location: cssparser::SourceLocation,
        name: CowRcStr<'i>,
    ) -> Result<HdPseudoElement, ParseError<'i, CssValidationError>> {
        Ok(HdPseudoElement(name.to_string()))
    }

    fn parse_functional_pseudo_element<'t>(
        &self,
        name: CowRcStr<'i>,
        parser: &mut CssParser<'i, 't>,
    ) -> Result<HdPseudoElement, ParseError<'i, CssValidationError>> {
        Err(parser.new_custom_error(CssValidationError::msg(format!(
            "unsupported functional pseudo-element ::{name}()"
        ))))
    }
}

// ---------------------------------------------------------------------------
// Selector scope validation (walks the parsed AST)
// ---------------------------------------------------------------------------

fn validate_selector_list(list: &SelectorList<HdSelectorImpl>) -> Result<(), String> {
    for selector in list.slice() {
        validate_selector(selector)?;
    }
    Ok(())
}

fn validate_selector(selector: &Selector<HdSelectorImpl>) -> Result<(), String> {
    // Parse order (left-to-right).
    let components: Vec<&Component<HdSelectorImpl>> =
        selector.iter_raw_parse_order_from(0).collect();
    if components.is_empty() {
        return Err("empty selector".to_string());
    }
    match components[0] {
        Component::ID(id) if id.as_str() == "root" => {}
        _ => {
            return Err(
                "selector must begin with #root (bare html/body/:root and other out-of-scope selectors are not allowed)"
                    .to_string(),
            )
        }
    }
    for component in &components {
        validate_component(component)?;
    }
    Ok(())
}

fn validate_component(component: &Component<HdSelectorImpl>) -> Result<(), String> {
    match component {
        Component::Combinator(combinator) => match combinator {
            Combinator::Child | Combinator::Descendant | Combinator::PseudoElement => Ok(()),
            Combinator::NextSibling | Combinator::LaterSibling => {
                Err("sibling combinators are not allowed (could escape #root)".to_string())
            }
            _ => Err("unsupported combinator".to_string()),
        },
        Component::Is(list) | Component::Where(list) | Component::Negation(list) => {
            validate_selector_list(list)
        }
        Component::NthOf(data) => {
            for selector in data.selectors() {
                validate_selector(selector)?;
            }
            Ok(())
        }
        // Simple selectors that can only match inside (or at) #root are fine.
        Component::ID(_)
        | Component::Class(_)
        | Component::LocalName(_)
        | Component::AttributeInNoNamespaceExists { .. }
        | Component::AttributeInNoNamespace { .. }
        | Component::AttributeOther(_)
        | Component::ExplicitUniversalType
        | Component::ExplicitAnyNamespace
        | Component::ExplicitNoNamespace
        | Component::DefaultNamespace(_)
        | Component::Namespace(..)
        | Component::Empty
        | Component::Nth(_)
        | Component::NonTSPseudoClass(_)
        | Component::PseudoElement(_) => Ok(()),
        Component::Has(_) => Err(":has() is not supported in theme CSS".to_string()),
        Component::Root => Err(":root is not allowed; selectors must begin with #root".to_string()),
        Component::Scope | Component::ImplicitScope | Component::ParentSelector => {
            Err("scoped / nesting selectors are not allowed".to_string())
        }
        Component::Host(_) | Component::Slotted(_) | Component::Part(_) => {
            Err("shadow-DOM selectors are not allowed".to_string())
        }
        Component::Invalid(_) | Component::RelativeSelectorAnchor => {
            Err("invalid selector".to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Raw token helpers (URL rejection + value consumption)
// ---------------------------------------------------------------------------

/// Reject any URL-bearing token (functions named `url`, unquoted URLs, or bad
/// URLs) anywhere in the token stream, descending into all blocks/functions.
fn scan_for_urls<'i>(
    parser: &mut CssParser<'i, '_>,
) -> Result<(), ParseError<'i, CssValidationError>> {
    loop {
        let token = match parser.next_including_whitespace_and_comments() {
            Ok(t) => t.clone(),
            Err(_) => return Ok(()),
        };
        match &token {
            Token::UnquotedUrl(_) | Token::BadUrl(_) => {
                return Err(parser.new_custom_error(CssValidationError::msg(
                    "url() references are not allowed in theme CSS",
                )));
            }
            Token::Function(name) => {
                if name.eq_ignore_ascii_case("url") {
                    return Err(parser.new_custom_error(CssValidationError::msg(
                        "url() references are not allowed in theme CSS",
                    )));
                }
                parser.parse_nested_block(scan_for_urls)?;
            }
            Token::ParenthesisBlock | Token::SquareBracketBlock | Token::CurlyBracketBlock => {
                parser.parse_nested_block(scan_for_urls)?;
            }
            _ => {}
        }
    }
}

/// Public helper used by the asset loader: reject any URL-bearing token in a
/// CSS value fragment, returning a plain `String` error.
pub fn reject_url_tokens<'i>(parser: &mut CssParser<'i, '_>) -> Result<(), String> {
    scan_for_urls(parser).map_err(|error| error.to_string())
}

/// Validate a token/surface VALUE is a single safe color — exactly one
/// `rgb()`/`rgba()` function with numeric/percentage comma-separated components
/// and nothing else (no trailing tokens, no comments/whitespace escape, no URL
/// or image function, no `var()`/`calc()`). This is the syntax-aware boundary
/// that proves the value cannot trigger an external fetch when the official UI
/// substitutes it into `background: var(--dsw-alias-bg-base)`.
pub fn validate_token_value(value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        // The engine skips empty values; nothing to validate.
        return Ok(());
    }
    // A valid color value must be a single closed `rgb()`/`rgba()` function.
    // cssparser's `parse_nested_block` does not itself reject an unclosed
    // function (it treats EOF like the closing delimiter), so require the
    // closing `)` here to fail closed on `rgb(…` without a `)`.
    if !trimmed.ends_with(')') {
        return Err("token value must be a single closed color function".to_string());
    }
    let mut input = ParserInput::new(trimmed);
    let mut parser = CssParser::new(&mut input);
    parse_safe_color_value(&mut parser).map_err(|error| format!("unsafe token value: {error}"))?;
    // No trailing token of any kind (whitespace, comment, delimiter, …).
    if parser.next_including_whitespace_and_comments().is_ok() {
        return Err("token value must be exactly one color value (no trailing tokens)".to_string());
    }
    Ok(())
}

/// Parse exactly one color value (the leading token of the value). Only
/// `rgb()`/`rgba()` are accepted; every other function/token is rejected.
fn parse_safe_color_value<'i>(
    parser: &mut CssParser<'i, '_>,
) -> Result<(), ParseError<'i, CssValidationError>> {
    let token = parser.next_including_whitespace_and_comments()?.clone();
    match &token {
        Token::Function(name) if name.eq_ignore_ascii_case("rgb") => {
            parser.parse_nested_block(|p| parse_rgb_components(p, 3))?;
            Ok(())
        }
        Token::Function(name) if name.eq_ignore_ascii_case("rgba") => {
            parser.parse_nested_block(|p| parse_rgb_components(p, 4))?;
            Ok(())
        }
        Token::UnquotedUrl(_) | Token::BadUrl(_) => Err(parser.new_custom_error(
            CssValidationError::msg("url() is not allowed in a token value"),
        )),
        Token::Function(name) => Err(parser.new_custom_error(CssValidationError::msg(format!(
            "unsupported function '{name}' in a token value"
        )))),
        other => Err(parser.new_custom_error(CssValidationError::msg(format!(
            "unsupported token in a token value: {other:?}"
        )))),
    }
}

/// Parse comma-separated `Number`/`Percentage` color components. Rejects any
/// other token (functions, strings, urls, delimiters), a wrong component count,
/// and a trailing comma.
fn parse_rgb_components<'i>(
    parser: &mut CssParser<'i, '_>,
    expected: usize,
) -> Result<(), ParseError<'i, CssValidationError>> {
    let mut count = 0usize;
    loop {
        let token = match parser.next() {
            Ok(token) => token.clone(),
            Err(_) => break,
        };
        match &token {
            Token::Number { .. } | Token::Percentage { .. } => {}
            other => {
                return Err(parser.new_custom_error(CssValidationError::msg(format!(
                    "unsupported color component: {other:?}"
                ))));
            }
        }
        count += 1;
        if count > expected {
            return Err(
                parser.new_custom_error(CssValidationError::msg("too many color components"))
            );
        }
        if parser.is_exhausted() {
            break;
        }
        parser.expect_comma()?;
        if parser.is_exhausted() {
            return Err(
                parser.new_custom_error(CssValidationError::msg("trailing comma in color value"))
            );
        }
    }
    if count != expected {
        return Err(parser.new_custom_error(CssValidationError::msg(format!(
            "expected {expected} color components, got {count}"
        ))));
    }
    Ok(())
}

/// Consume every token up to the enclosing delimiter, descending into nested
/// blocks and functions. Used to skip at-rule preludes we do not need to
/// interpret further (already covered by the URL scan pass).
fn consume_component_values<'i>(
    parser: &mut CssParser<'i, '_>,
) -> Result<(), ParseError<'i, CssValidationError>> {
    loop {
        match parser.next_including_whitespace_and_comments() {
            Ok(token) => {
                if matches!(
                    token,
                    Token::ParenthesisBlock
                        | Token::SquareBracketBlock
                        | Token::CurlyBracketBlock
                        | Token::Function(_)
                ) {
                    parser.parse_nested_block(consume_component_values)?;
                }
            }
            Err(_) => return Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Declaration blocks (style rules + keyframe blocks)
// ---------------------------------------------------------------------------

struct DeclarationValidator;

impl<'i> DeclarationParser<'i> for DeclarationValidator {
    type Declaration = ();
    type Error = CssValidationError;

    fn parse_value<'t>(
        &mut self,
        _name: CowRcStr<'i>,
        input: &mut CssParser<'i, 't>,
        _declaration_start: &cssparser::ParserState,
    ) -> Result<(), ParseError<'i, CssValidationError>> {
        consume_component_values(input)
    }
}

// Nested qualified rules and at-rules inside a declaration block are rejected
// by the default trait implementations (they return `QualifiedRuleInvalid` /
// `AtRuleInvalid`).
impl<'i> QualifiedRuleParser<'i> for DeclarationValidator {
    type Prelude = ();
    type QualifiedRule = ();
    type Error = CssValidationError;
}

impl<'i> AtRuleParser<'i> for DeclarationValidator {
    type Prelude = ();
    type AtRule = ();
    type Error = CssValidationError;
}

impl<'i> RuleBodyItemParser<'i, (), CssValidationError> for DeclarationValidator {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

fn parse_declaration_block<'i>(
    input: &mut CssParser<'i, '_>,
) -> Result<(), ParseError<'i, CssValidationError>> {
    let mut declarations = DeclarationValidator;
    let body = RuleBodyParser::new(input, &mut declarations);
    for item in body {
        item.map_err(|(error, _)| error)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Keyframes
// ---------------------------------------------------------------------------

fn parse_keyframes_name<'i>(
    input: &mut CssParser<'i, '_>,
) -> Result<(), ParseError<'i, CssValidationError>> {
    let name = match input.expect_ident() {
        Ok(name) => name.clone(),
        Err(_) => {
            return Err(input.new_custom_error(CssValidationError::msg(
                "@keyframes requires an identifier name",
            )));
        }
    };
    if !name.to_ascii_lowercase().starts_with("hd-") {
        return Err(input.new_custom_error(CssValidationError::msg(format!(
            "@keyframes name '{name}' must use the 'hd-' prefix"
        ))));
    }
    consume_component_values(input)
}

struct KeyframesParser;

impl<'i> QualifiedRuleParser<'i> for KeyframesParser {
    type Prelude = ();
    type QualifiedRule = ();
    type Error = CssValidationError;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut CssParser<'i, 't>,
    ) -> Result<(), ParseError<'i, CssValidationError>> {
        // Keyframe selectors are `from`, `to`, or a comma-separated list of
        // percentages — keyframe grammar, never DOM selectors.
        input.parse_entirely(|input| {
            while !input.is_exhausted() {
                let location = input.current_source_location();
                match input.next() {
                    Ok(Token::Ident(name))
                        if name.eq_ignore_ascii_case("from")
                            || name.eq_ignore_ascii_case("to") => {}
                    Ok(Token::Percentage { .. }) => {}
                    Ok(other) => {
                        return Err(location.new_custom_error(CssValidationError::msg(format!(
                            "invalid @keyframes selector: expected 'from', 'to', or a percentage, got {other:?}"
                        ))));
                    }
                    Err(_) => {
                        return Err(location.new_custom_error(CssValidationError::msg(
                            "invalid @keyframes selector",
                        )));
                    }
                }
                if input.is_exhausted() {
                    break;
                }
                input.expect_comma()?;
            }
            Ok(())
        })
    }

    fn parse_block<'t>(
        &mut self,
        _prelude: (),
        _start: &cssparser::ParserState,
        input: &mut CssParser<'i, 't>,
    ) -> Result<(), ParseError<'i, CssValidationError>> {
        parse_declaration_block(input)
    }
}

impl<'i> AtRuleParser<'i> for KeyframesParser {
    type Prelude = ();
    type AtRule = ();
    type Error = CssValidationError;
}

fn parse_keyframes_block<'i>(
    input: &mut CssParser<'i, '_>,
) -> Result<(), ParseError<'i, CssValidationError>> {
    let mut keyframes = KeyframesParser;
    let sheet = StyleSheetParser::new(input, &mut keyframes);
    for item in sheet {
        item.map_err(|(error, _)| error)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Top-level rule list
// ---------------------------------------------------------------------------

enum AtRulePrelude {
    Group,
    Keyframes,
}

struct Validator;

impl<'i> QualifiedRuleParser<'i> for Validator {
    type Prelude = ();
    type QualifiedRule = ();
    type Error = CssValidationError;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut CssParser<'i, 't>,
    ) -> Result<(), ParseError<'i, CssValidationError>> {
        let list = SelectorList::parse(&HdSelectorParser, input, ParseRelative::No)?;
        validate_selector_list(&list)
            .map_err(|message| input.new_custom_error(CssValidationError(message)))?;
        Ok(())
    }

    fn parse_block<'t>(
        &mut self,
        _prelude: (),
        _start: &cssparser::ParserState,
        input: &mut CssParser<'i, 't>,
    ) -> Result<(), ParseError<'i, CssValidationError>> {
        parse_declaration_block(input)
    }
}

impl<'i> AtRuleParser<'i> for Validator {
    type Prelude = AtRulePrelude;
    type AtRule = ();
    type Error = CssValidationError;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut CssParser<'i, 't>,
    ) -> Result<AtRulePrelude, ParseError<'i, CssValidationError>> {
        if name.eq_ignore_ascii_case("media") || name.eq_ignore_ascii_case("supports") {
            consume_component_values(input)?;
            return Ok(AtRulePrelude::Group);
        }
        if name.eq_ignore_ascii_case("keyframes") {
            parse_keyframes_name(input)?;
            return Ok(AtRulePrelude::Keyframes);
        }
        if name.eq_ignore_ascii_case("import") {
            return Err(input.new_custom_error(CssValidationError::msg(
                "@import is not allowed in theme CSS",
            )));
        }
        if name.eq_ignore_ascii_case("font-face") {
            return Err(input.new_custom_error(CssValidationError::msg(
                "@font-face is not allowed in theme CSS",
            )));
        }
        Err(input.new_custom_error(CssValidationError::msg(format!(
            "unsupported at-rule @{name}"
        ))))
    }

    fn parse_block<'t>(
        &mut self,
        prelude: AtRulePrelude,
        _start: &cssparser::ParserState,
        input: &mut CssParser<'i, 't>,
    ) -> Result<(), ParseError<'i, CssValidationError>> {
        match prelude {
            AtRulePrelude::Group => {
                let sheet = StyleSheetParser::new(input, self);
                for item in sheet {
                    item.map_err(|(error, _)| error)?;
                }
                Ok(())
            }
            AtRulePrelude::Keyframes => parse_keyframes_block(input),
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Validate a `components` or `motion` CSS string. Returns `Ok(())` when the
/// CSS is provably confined to `#root` (and otherwise harmless), or `Err` with
/// a clear message. Fails closed: any parse or scope violation is an error.
pub fn validate_theme_css(css: &str, what: &str) -> Result<(), String> {
    if css.trim().is_empty() {
        return Ok(());
    }

    // Pass 1: reject any URL-bearing token anywhere in the input (including
    // inside custom-property values, keyframe bodies, and nested blocks).
    {
        let mut input = ParserInput::new(css);
        let mut parser = CssParser::new(&mut input);
        if let Err(error) = scan_for_urls(&mut parser) {
            return Err(format!("{what}: {error}"));
        }
    }

    // Pass 2: structural + scope validation.
    let mut input = ParserInput::new(css);
    let mut parser = CssParser::new(&mut input);
    let mut validator = Validator;
    let sheet = StyleSheetParser::new(&mut parser, &mut validator);
    for item in sheet {
        if let Err((error, _slice)) = item {
            return Err(format!("{what}: {error}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_theme_css;

    fn ok(css: &str) {
        validate_theme_css(css, "test")
            .unwrap_or_else(|e| panic!("expected valid, got: {e}\ncss: {css}"));
    }

    fn err(css: &str) -> String {
        validate_theme_css(css, "test").expect_err(&format!("expected invalid, got Ok\ncss: {css}"))
    }

    #[test]
    fn valid_root_selector() {
        ok("#root .foo { color: red; }");
        ok("#root > * { animation: x 1s; }");
        ok("#root, #root .a { border-radius: 2px; }");
    }

    #[test]
    fn valid_nested_root_selector() {
        ok("#root .a .b > .c { color: red; }");
    }

    #[test]
    fn is_where_valid() {
        ok("#root :is(#root .a, #root .b) { color: red; }");
        ok("#root :where(#root .a, #root .b) { color: red; }");
    }

    #[test]
    fn is_where_invalid() {
        err("#root :is(.a, .b) { color: red; }");
        err("#root :where(html, body) { color: red; }");
        err(":is(#root .a, #root .b) { color: red; }");
    }

    #[test]
    fn not_valid_and_invalid() {
        ok("#root :not(#root .a) { color: red; }");
        err("#root :not(.a) { color: red; }");
    }

    #[test]
    fn nth_child_of_valid_and_invalid() {
        ok("#root :nth-child(2 of #root .a) { color: red; }");
        ok("#root :nth-last-child(1 of #root .a) { color: red; }");
        err("#root :nth-child(2 of .a) { color: red; }");
        err("#root :nth-last-child(1 of .a) { color: red; }");
    }

    #[test]
    fn has_rejected() {
        err("#root:has(.a) { color: red; }");
        err("#root :has(.a) { color: red; }");
    }

    #[test]
    fn bare_root_selectors_rejected() {
        err("html { color: red; }");
        err("body { color: red; }");
        err(":root { color: red; }");
        err("div { color: red; }");
    }

    #[test]
    fn cross_root_selector_rejected() {
        err("body #root { color: red; }");
        err("#root2 .a { color: red; }");
    }

    #[test]
    fn sibling_escape_rejected() {
        err("#root + .a { color: red; }");
        err("#root ~ .a { color: red; }");
        err("#root .a + .b { color: red; }");
    }

    #[test]
    fn media_recursive_validation() {
        ok("@media (max-width: 600px) { #root .a { color: red; } }");
        err("@media (max-width: 600px) { .a { color: red; } }");
    }

    #[test]
    fn supports_recursive_validation() {
        ok("@supports (display: grid) { #root .a { color: red; } }");
        err("@supports (display: grid) { .a { color: red; } }");
    }

    #[test]
    fn unknown_at_rule_rejected() {
        err("@foo { #root .a { color: red; } }");
    }

    #[test]
    fn import_and_font_face_rejected() {
        err("@import url(\"https://example.com/x.css\");");
        err("@import \"https://example.com/x.css\";");
        err("@font-face { font-family: x; src: url(\"x.woff2\"); }");
    }

    #[test]
    fn keyframes_hd_accepted() {
        ok("@keyframes hd-whale-float { from { opacity: 0; } to { opacity: 1; } }");
        ok("@keyframes hd-x { 0% { top: 0; } 50% { top: 10px; } 100% { top: 0; } }");
    }

    #[test]
    fn keyframes_non_hd_rejected() {
        err("@keyframes whale-float { from { opacity: 0; } to { opacity: 1; } }");
    }

    #[test]
    fn keyframes_from_to_percent_parse() {
        ok("@keyframes hd-x { from, 50%, to { opacity: 0.5; } }");
        // Keyframe selectors must NOT be forced to start with #root.
        ok("@keyframes hd-x { to { opacity: 1; } }");
    }

    #[test]
    fn keyframes_invalid_selector_rejected() {
        err("@keyframes hd-x { #root .a { opacity: 0; } }");
    }

    #[test]
    fn url_rejected() {
        err("#root .a { background: url(\"https://example.com/x.png\"); }");
        err("#root .a { background: url(data:image/png;base64,AAAA); }");
        err("#root .a { --x: url(https://example.com/x.png); }");
    }

    #[test]
    fn ocean_and_starter_components_motion_pass() {
        let ocean = include_str!("../../themes/ocean/theme.json");
        let starter = include_str!("../../themes/starter/theme.json");
        let ocean_theme: crate::appearance::Theme = serde_json::from_str(ocean).unwrap();
        let starter_theme: crate::appearance::Theme = serde_json::from_str(starter).unwrap();
        validate_theme_css(ocean_theme.components.as_deref().unwrap(), "components").unwrap();
        validate_theme_css(ocean_theme.motion.as_deref().unwrap(), "motion").unwrap();
        validate_theme_css(starter_theme.components.as_deref().unwrap(), "components").unwrap();
        validate_theme_css(starter_theme.motion.as_deref().unwrap(), "motion").unwrap();
    }

    #[test]
    fn deep_glass_components_motion_pass() {
        let deep_glass = include_str!("../../themes/deep-glass/theme.json");
        let theme: crate::appearance::Theme = serde_json::from_str(deep_glass).unwrap();
        validate_theme_css(theme.components.as_deref().unwrap(), "components").unwrap();
        validate_theme_css(theme.motion.as_deref().unwrap(), "motion").unwrap();
    }
}
