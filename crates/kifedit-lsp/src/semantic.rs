use std::collections::HashMap;

use kifedit_project::Project;
use kifedit_reason::Kinds;
use kifedit_sema::{FileId, Symbol};
use kifedit_syntax::SyntaxKind;
use tower_lsp::lsp_types::{SemanticToken, SemanticTokenType, SemanticTokensLegend};

use crate::line_index::LineIndex;

// 索引即 SemanticToken.token_type；使用標準型別，佈景主題才有顏色
const TYPES: [SemanticTokenType; 4] = [
    SemanticTokenType::CLASS,       // class
    SemanticTokenType::FUNCTION,    // relation
    SemanticTokenType::MACRO,       // function（…Fn）
    SemanticTokenType::ENUM_MEMBER, // individual、attribute
];

pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TYPES.to_vec(),
        token_modifiers: Vec::new(),
    }
}

/// 一個詞可能同時有多種身分（例如關係也是 individual），取最具體的一種
fn token_type(kinds: Kinds) -> Option<u32> {
    if kinds.function {
        Some(2)
    } else if kinds.relation {
        Some(1)
    } else if kinds.class {
        Some(0)
    } else if kinds.attribute || kinds.individual {
        Some(3)
    } else {
        None
    }
}

/// 整份文件的語意標記；變數、字串、註解由 TextMate 語法標色
pub fn tokens(project: &Project, file: FileId) -> Vec<SemanticToken> {
    let (Some(parse), Some(text)) = (project.parse(file), project.text(file)) else {
        return Vec::new();
    };
    let index = LineIndex::new(text);
    let interner = project.interner();
    let taxonomy = project.taxonomy();
    let mut types: HashMap<Symbol, Option<u32>> = HashMap::new();

    let mut out = Vec::new();
    let (mut line, mut start) = (0, 0);
    for token in parse
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        if token.kind() != SyntaxKind::Word {
            continue;
        }
        let Some(symbol) = interner.get(token.text()) else {
            continue;
        };
        let Some(token_type) = *types
            .entry(symbol)
            .or_insert_with(|| token_type(taxonomy.kinds(symbol)))
        else {
            continue;
        };
        let position = index.position(token.text_range().start().into());
        let delta_line = position.line - line;
        out.push(SemanticToken {
            delta_line,
            delta_start: if delta_line == 0 {
                position.character - start
            } else {
                position.character
            },
            length: token.text().encode_utf16().count() as u32,
            token_type,
            token_modifiers_bitset: 0,
        });
        (line, start) = (position.line, position.character);
    }
    out
}
