#[cfg(test)]
mod tests;

use crate::{
    lexer::{Error as LexError, TokenKind},
    parser::{
        expression::BindingIdentifier,
        function::{FormalParameters, FunctionBody},
        name_in_lexically_declared_names, Cursor, OrAbrupt, ParseResult, TokenParser,
    },
    source::ReadChar,
    Error,
};
use boa_ast::{
    function::AsyncFunctionExpression as AsyncFunctionExpressionNode,
    operations::{bound_names, contains, lexically_declared_names, ContainsSymbol},
    Keyword, Punctuator, Span,
};
use boa_interner::{Interner, Sym};

/// Async Function expression parsing.
///
/// More information:
///  - [MDN documentation][mdn]
///  - [ECMAScript specification][spec]
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/async_function
/// [spec]: https://tc39.es/ecma262/#prod-AsyncFunctionExpression
#[derive(Debug, Clone, Copy)]
pub(super) struct AsyncFunctionExpression {}

impl AsyncFunctionExpression {
    /// Creates a new `AsyncFunctionExpression` parser.
    pub(super) fn new() -> Self {
        Self {}
    }
}

impl<R> TokenParser<R> for AsyncFunctionExpression
where
    R: ReadChar,
{
    type Output = AsyncFunctionExpressionNode;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        let token = cursor.expect(
            (Keyword::Async, false),
            "async function expression",
            interner,
        )?;
        let start_linear_span = token.linear_span();
        let function_span_start = token.span().start();

        cursor.peek_expect_no_lineterminator(0, "async function expression", interner)?;
        cursor.expect(
            (Keyword::Function, false),
            "async function expression",
            interner,
        )?;

        let token = cursor.peek(0, interner).or_abrupt()?;
        let (name, name_span) = match token.kind() {
            TokenKind::IdentifierName(_)
            | TokenKind::Keyword((
                Keyword::Yield | Keyword::Await | Keyword::Async | Keyword::Of,
                _,
            )) => {
                let span = token.span();
                let name = BindingIdentifier::new(false, true).parse(cursor, interner)?;

                (Some(name), span)
            }
            _ => (None, token.span()),
        };

        let params_start_position = cursor
            .expect(Punctuator::OpenParen, "async function expression", interner)?
            .span()
            .end();

        let params = FormalParameters::new(false, true).parse(cursor, interner)?;

        cursor.expect(
            Punctuator::CloseParen,
            "async function expression",
            interner,
        )?;

        let body =
            FunctionBody::new(false, true, "async function expression").parse(cursor, interner)?;

        // Early Error: If the source code matching FormalParameters is strict mode code,
        // the Early Error rules for UniqueFormalParameters : FormalParameters are applied.
        if (cursor.strict() || body.strict()) && params.has_duplicates() {
            return Err(Error::lex(LexError::Syntax(
                "Duplicate parameter name not allowed in this context".into(),
                params_start_position,
            )));
        }

        // Early Error: It is a Syntax Error if FunctionBodyContainsUseStrict of AsyncFunctionBody is true
        // and IsSimpleParameterList of FormalParameters is false.
        if body.strict() && !params.is_simple() {
            return Err(Error::lex(LexError::Syntax(
                "Illegal 'use strict' directive in function with non-simple parameter list".into(),
                params_start_position,
            )));
        }

        // Early Error: If BindingIdentifier is present and the source code matching BindingIdentifier is strict mode code,
        // it is a Syntax Error if the StringValue of BindingIdentifier is "eval" or "arguments".
        if let Some(name) = name {
            if (cursor.strict() || body.strict())
                && [Sym::EVAL, Sym::ARGUMENTS].contains(&name.sym())
            {
                return Err(Error::lex(LexError::Syntax(
                    "unexpected identifier 'eval' or 'arguments' in strict mode".into(),
                    name_span.start(),
                )));
            }
        }

        // Catch early error for BindingIdentifier, because strictness of the functions body is also
        // relevant for the function parameters.
        if body.strict() && contains(&params, ContainsSymbol::EvalOrArguments) {
            return Err(Error::lex(LexError::Syntax(
                "unexpected identifier 'eval' or 'arguments' in strict mode".into(),
                params_start_position,
            )));
        }

        // It is a Syntax Error if any element of the BoundNames of FormalParameters
        // also occurs in the LexicallyDeclaredNames of FunctionBody.
        // https://tc39.es/ecma262/#sec-function-definitions-static-semantics-early-errors
        name_in_lexically_declared_names(
            &bound_names(&params),
            &lexically_declared_names(&body),
            params_start_position,
            interner,
        )?;

        let span = start_linear_span.union(body.linear_pos_end());

        let function_span_end = body.span().end();
        let function = AsyncFunctionExpressionNode::new(
            name,
            params,
            body,
            span,
            name.is_some(),
            Span::new(function_span_start, function_span_end),
        );

        if contains(&function, ContainsSymbol::Super) {
            return Err(Error::lex(LexError::Syntax(
                "invalid super usage".into(),
                params_start_position,
            )));
        }

        Ok(function)
    }
}
