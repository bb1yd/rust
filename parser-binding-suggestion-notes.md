# rustc_parse：绑定信息可改善的诊断建议

> **个人调研笔记，LLM 生成，仅供本地阅读，不用于上游贡献。**
> 本文整理自本次会话对源码及已有 UI 快照的只读调研，没有运行测试或性能基准。现有快照展示的行为与根据源码推断的优化机会分别标注，不把候选方向当成已经验证的 bug。
>
> 按项目 `AGENTS.md` 的 Prohibited text、Reviewer 规则及个人使用豁免约束，本文不能直接或润色后作为上游文档、PR/issue 正文或公开评论提交；相关公开文本须由人独立撰写。若改变用途，应重新核对规则。参见 [Rust LLM policy](https://forge.rust-lang.org/policies/llm-usage.html) 和 [dev-guide 的 LLM guidance](https://rustc-dev-guide.rust-lang.org/llm-guidance/writing.html)。

## 1. 背景与范围

maintainer 提出的方向是：记录当前 item 引入的 bindings，只在合理时建议把 identifier 当作 value，例如把 keyword 改为 raw identifier。代价是 parser 内引入一套不完整的名称解析，以及额外的内存和复制成本。

本文重点不是列出所有 parser 错误，而是寻找：

- 建议需要某个名字作为值，但目前没有证据表明该值存在的地方；
- 已有局部值可能被 token 启发式误判为其他语法的地方；
- 局部值信息能排除明显不成立的修复方向的地方；
- 可以复用绑定信息、但不属于原提议核心的旁支收益。

源码链接以文件和函数为主。最初调研基于引入绑定记录前的版本；当前分支已有 `e29bfa5d6ef`（`Record local bindings in parser.`），部分条件与行号已经变化。阅读时应结合该提交的 diff，尤其是 `expected_ident_found`。

## 2. 候选概览

| 优先级 | 诊断／恢复方向 | 绑定信息的价值 | 证据强度 |
| --- | --- | --- | --- |
| 高 | `not` 被恢复为否定运算符 | 避免把已声明的值误判成外语式运算符 | 已有测试和快照直接展示反例 |
| 高 | 索引失败时建议补宏调用的 `!` | 区分真实索引与漏写 `!` 的宏调用意图 | 条件宽泛已确认；已绑定变量的误报尚未验证 |
| 高 | 模式表达式提取为 `const` | 局部值引用可排除无法捕获局部变量的 `const` 建议 | 源码和现有建议已确认；绑定过滤需进一步验证 |
| 基准场景 | 字段简写中的 raw identifier 建议 | 简写既需要字段名，也需要同名值 | 已有竞争建议快照 |
| 中 | `let x += ...` 的初始化／更新旧值方向 | 已有旧绑定可增强更新意图的可信度 | 已有快照展示无旧绑定时仍给 overwrite help |
| 中 | 链式比较与遗漏 turbofish | 已知局部值可增强比较意图的证据 | 源码启发式已确认；暂无直接绑定误报用例 |
| 探索 | 普通 `let` 中的字段／索引表达式 | 已有 receiver 可支持“误加 `let`”的解释 | 已有表达式模式错误覆盖，修复意图仍不确定 |
| 审计点 | `if let` 与 `let…else` 的选择 | 改善现有同名 identifier 启发式的可靠性 | 已有绑定相关启发式与测试 |
| 旁支 | 建议引入的临时名字避碰 | 避免 `tmp`、`val` 等名字遮蔽原表达式中的值 | 已有固定命名逻辑；一般冲突尚未验证 |

## 3. 字段简写中的 keyword → raw identifier

### 位置

- [`SuggEscapeIdentifier`](compiler/rustc_parse/src/diagnostics.rs)：建议定义。
- [`expected_ident_found`](compiler/rustc_parse/src/parser/diagnostics.rs)：生成建议及恢复 identifier。
- [`parse_expr_field`](compiler/rustc_parse/src/parser/expr.rs)：字段简写与显式字段的分流。
- 同文件的 `parse_struct_fields`：结构体解析失败后的竞争恢复。

### 当前问题与收益

字段简写 `S { field }` 会把 `field` 同时用作字段名和 `ExprKind::Path` 值。因此，建议把 keyword 改成 raw identifier 时，同名值是否合理存在是有用证据。

典型噪声是把缺少 `else` 的 `foo{return;}` 按结构体初始化解析，然后同时建议 `r#return` 和补 `else`。

### 已有覆盖

- [`detect-missing-else-in-let-1.rs`](tests/ui/parser/detect-missing-else-in-let-1.rs) 及其 [快照](tests/ui/parser/detect-missing-else-in-let-1.stderr)：`foo{return;}` 同时得到 raw identifier 和 `let-else` 方向的建议。
- [`detect-missing-else-in-let-2.rs`](tests/ui/parser/detect-missing-else-in-let-2.rs)：相关恢复场景。
- [`issue-15980.rs`](tests/ui/parser/issues/issue-15980.rs) 及其 [快照](tests/ui/parser/issues/issue-15980.stderr)：match guard 后缺少 `=>`，同时出现 `r#return` 和补箭头的建议。

### 边界

不能把局部绑定 membership 作为所有 identifier 建议的共同前提：

- `S { field: expr }` 中的字段名不要求存在同名变量；
- 函数名、字段声明、新绑定名等声明位置也不要求名字已经存在；
- 字段简写中的值还可能来自常量、函数或导入值，不一定是局部绑定。

当前分支已在通用 `expected_ident_found` 中接入集合查询；上述区分仍然重要。

## 4. `not` 被误认成否定运算符

### 位置与触发

[`expr.rs`](compiler/rustc_parse/src/parser/expr.rs) 中：

- `parse_expr_prefix`；
- `is_mistaken_not_ident_negation`；
- `recover_not_expr`。

当前启发式检查名字是否为 `not`，以及后一个 token 是否能开始否定操作数、却不能直接续接普通 identifier。随后依据操作数是否为数字或布尔字面量选择建议分支。

### 已确认的反例

[`issue-46836-identifier-not-instead-of-negation.rs`](tests/ui/did_you_mean/issue-46836-identifier-not-instead-of-negation.rs) 的 `should_we` 先声明 `let not = true`，随后在 `if not` 后漏写花括号。

对应 [快照](tests/ui/did_you_mean/issue-46836-identifier-not-instead-of-negation.stderr) 仍建议把 `not` 改成 `!`，随后再报告条件后缺少块。

### 绑定信息的价值与限制

已存在的局部值 `not` 是很强的反证：缺少条件块比错误使用外语式运算符更值得考虑。收益不仅是少一条建议，也可能减少错误恢复方向造成的连锁诊断。

但绑定存在仍不能证明用户绝不可能把 `not` 用作否定运算符，需要结合语法上下文。同一测试还覆盖真正应当恢复为否定运算符的情况。

## 5. 索引失败后建议补 `!`

### 位置与触发

[`parse_expr_index`](compiler/rustc_parse/src/parser/expr.rs)。

解析 `base[...]` 时，索引表达式后未遇到 `]`，只要 `[` 前的 token 是非保留 identifier，就可能建议补宏调用的 `!`。该条件没有检查 base 是否对应已知局部变量，也没有确认同名宏存在。

### 已有覆盖

- [`macro/do-not-suggest-semicolon-between-macro-without-exclamation-mark-and-array.rs`](tests/ui/parser/macro/do-not-suggest-semicolon-between-macro-without-exclamation-mark-and-array.rs)：`vec[1, 2, 3]`，是保留宏调用恢复能力的正面例子。
- [`do-not-suggest-semicolon-before-index.rs`](tests/ui/parser/do-not-suggest-semicolon-before-index.rs) 及其 [快照](tests/ui/parser/do-not-suggest-semicolon-before-index.stderr)：`indexee[...]` 内出现逗号，也得到补 `!` 的建议。

### 绑定信息的价值与限制

如果 base 明确是参数或局部变量，真正的索引内容写错这一解释会更可信。

不过，值和宏属于不同命名空间，可以同名。绑定信息只能影响置信度，不能证明宏调用意图不可能。上述 `indexee` 测试没有声明该变量，因此尚不是“已绑定变量被误判”的直接测试。

## 6. 模式表达式的 `const` 提取建议

### 位置与触发

[`pat.rs`](compiler/rustc_parse/src/parser/pat.rs) 中：

- `maybe_recover_trailing_expr`：把模式后缀恢复成表达式，暂存错误；
- `maybe_augment_stashed_expr_in_pats_with_suggestions`：结合语句上下文补建议；
- 内部的 `maybe_add_suggestions_then_emit`：添加 guard 或 `const` 提取建议。

诊断定义为 [`UnexpectedExpressionInPatternSugg`](compiler/rustc_parse/src/diagnostics.rs)。

字段访问、索引、方法调用、运算、类型转换等出现在模式中时，可能同时得到移入 guard 和提取为 `const` 的建议。普通 `let` 的声明／初始化分支目前提前返回，不补这些建议；`let-else` 等上下文仍可能得到建议。

### 绑定信息的价值

如果表达式中的值引用确实指向局部变量或函数参数，新建的 `const` item 不能捕获该值。因此可以排除明显不成立的提取方向。

没有局部绑定证据则不能反推表达式可用于常量求值：非 const 调用等仍可能使它无效。

### 已有覆盖与边界

- [`recover-pat-exprs.rs`](tests/ui/parser/recover/recover-pat-exprs.rs) 及其 [快照](tests/ui/parser/recover/recover-pat-exprs.stderr)：广泛覆盖字段、索引、调用和运算，以及同时给 guard／`const` 建议的行为。其中不少名字没有外层绑定，不是局部引用过滤的直接覆盖。
- 同一文件的 `1 + 1` 是保留常量方向的对照。
- [`recover-pat-issues.rs`](tests/ui/parser/recover/recover-pat-issues.rs) 中的 `MAGIC` 是函数内的 `const` item，不是局部变量，不能因为在函数中声明就排除。
- [`recover-pat-ranges.rs`](tests/ui/parser/recover/recover-pat-ranges.rs)：range bound 的相关建议路径。

恢复过程以 `ExprKind::Dummy` 代替已解析的模式前缀，随后主要保留 span 和优先级信息。因此现有错误模式 AST 并不总能提供完整的原表达式和值引用，仅增加名字集合还不够。

## 7. `let x += ...` 的初始化／更新方向

### 位置与触发

- [`parse_initializer`](compiler/rustc_parse/src/parser/stmt.rs)。
- [`CompoundAssignmentExpressionInLet`](compiler/rustc_parse/src/diagnostics.rs)。

声明初始化器位置遇到复合赋值 token 时，当前按普通初始化恢复，建议去掉额外运算符，同时附带“若想覆盖则移除 `let`”方向的 help。

### 绑定信息的价值

对简单标识符，如果错误声明之前已经有可见的同名绑定，更新旧值的意图更可信。查询不能把当前错误声明正在引入的名字算进去。

### 已有覆盖与限制

[`let-binop.rs`](tests/ui/parser/let-binop.rs) 的 `a/b/c` 此前没有局部绑定，但 [快照](tests/ui/parser/let-binop.stderr) 都包含 overwrite help；现有 rustfix 选择初始化方向。

绑定存在不证明可变性、已初始化或运算符适用性，也不排除遮蔽意图。带类型注解的声明不能只删 `let` 就成为合法赋值；复杂模式也不等于简单变量。因此不足以据此提高为自动修复。

## 8. 链式比较与遗漏 turbofish

### 位置与触发

[`diagnostics.rs`](compiler/rustc_parse/src/parser/diagnostics.rs) 中：

- `check_no_chained_comparison`；
- `attempt_chained_comparison_suggestion`；
- `check_mistyped_turbofish_with_multiple_type_params`。

最值得关注的是 `check_no_chained_comparison` 的弱启发式：对于形似 `foo < bar > ...` 的输入，没有调用／关联路径等更强后缀证据时，只要前两个操作数都不是字面量，就可能附带 turbofish 方向的 help。

### 绑定信息的价值

若操作数是已知的当前可见局部值，确实在比较值的解释更可信，可考虑减少无关泛型提示。

多泛型参数恢复分支也有潜在过滤价值，但它不是现成的 comparison／turbofish 二选一：该分支先遇到解析错误，再尝试解析后续泛型实参。目前证据更弱。

### 已有覆盖与限制

- [`require-parens-for-chained-comparison.rs`](tests/ui/parser/require-parens-for-chained-comparison.rs) 及其 [快照](tests/ui/parser/require-parens-for-chained-comparison.stderr)：覆盖具体 turbofish 修正、只给帮助和多参数恢复。
- [`chained-comparison-suggestion.rs`](tests/ui/parser/chained-comparison-suggestion.rs)：覆盖比较拆分／括号选择。
- [`bastion-of-the-turbofish.rs`](tests/ui/parser/bastion-of-the-turbofish.rs)：合法比较表达式的重要边界。

类型和值可能同名，泛型实参也可能是 const 参数。不能因为某名字是值，就认定它不可能出现在泛型实参中。此次没有找到局部绑定直接导致错误 turbofish 建议的现成测试，不应称为已复现误报。

## 9. 其他收益与审计点

### 9.1 普通 `let` 中的表达式／place 混淆

[`pat.rs`](compiler/rustc_parse/src/parser/pat.rs) 对普通 `let` 中恢复出的表达式模式错误通常不补进一步建议。

若表达式根部是已有绑定，字段或索引 place 可能支持“误把赋值写成声明”的解释。但不能扩展到所有表达式：方法调用不是可赋值 place，receiver 存在也不证明字段、索引或类型正确。

[`recover-pat-lets.rs`](tests/ui/parser/recover/recover-pat-lets.rs) 包含先声明 `x` 后又出现方法调用、类型注解、索引等情况。测试中的 `x` 是 `Option`，不能据此证明去掉 `let` 就语义正确。

### 9.2 `if let` 与 `let…else` 的选择

[`recover_missing_let_else`](compiler/rustc_parse/src/parser/stmt.rs) 已收集模式中的名字，并遍历随后 block，按是否出现同名 identifier 和 `return` 来选择建议及 applicability。

同名 identifier 不一定引用该模式绑定：外层同名值、block 内新绑定、字段名、嵌套作用域等都可能干扰。宏展开前也看不到所有引用。

[`missing-if-let-or-let-else.rs`](tests/ui/uninhabited/missing-if-let-or-let-else.rs) 覆盖直接引用、`return` 和格式字符串中的宏引用。这更适合视为现有启发式的可靠性审计点，而不是简单新增 membership 判断。

### 9.3 建议引入名字时避碰

[`report_inc_dec_op`](compiler/rustc_parse/src/parser/expr/diagnostics.rs) 的后缀自增／自减建议使用临时名字 `tmp`，只在整个操作数字符串恰好等于 `tmp` 时换成 `tmp_`。

复合操作数内部也可能引用同名值，新临时绑定可能影响建议中后续重复的原表达式。绑定信息可用于名字避碰，但这不属于原提议的值／非值意图判断。

[`increment-autofix-2.rs`](tests/ui/parser/increment-autofix-2.rs) 覆盖普通名字及操作数本身叫 `tmp` 的情况，不代表一般复合操作数冲突已覆盖。该后缀表达式建议是 `HasPlaceholders`，不能与其他分支的 `MachineApplicable` 混为一谈。

模式转 guard 使用的 `val` 或字段名也有类似的名字避碰审计价值，但此次没有验证具体冲突用例。

## 10. 不宜直接套用局部绑定过滤的方向

- **补 `let`、`var`／`auto` 改 `let`**：已有同名绑定不是反证，因为遮蔽合法。[`missing-let-in-binding.rs`](tests/ui/type/missing-let-in-binding.rs) 本身就包含已有同名绑定。
- **显式字段名、成员名、新声明名**：不要求存在同名局部值。
- **`and`／`or` 运算符恢复**：已有同名绑定不会使 `a and b` 成为合法表达式，单靠集合不足以推翻运算符位置的语法证据。
- **字符、lifetime、label 恢复**：需要对应语法或命名空间的信息，局部值集合不能替代。
- **`try!` 的 raw identifier 建议**：这是有明确 token 形状和特定宏背景的恢复，不应等同于局部值查找。
- **一般 missing-dot、item 或参数恢复**：需要更具体的语法和语义证据，未发现仅靠绑定集合就值得优先推进的强候选。

## 11. 共同约束与成本

### “出现过”不等于“可见且被引用”

需要区分：

1. 当前 item 中出现过这个名字；
2. 当前错误位置有可见的同名绑定；
3. 当前表达式确实引用该绑定。

前一个 match arm、已结束的 block、当前 `let` 自己的 RHS、`else`、closure 和嵌套 item 都会影响上述判断。模式 visitor 还不能把字段名、路径段、方法名当作值绑定。

集合中缺名不是名称不存在的证明：值还可能来自导入、常量、函数或宏展开。即便只检查 `PatKind::Ident`，parser 也无法完全区分局部绑定与常量／unit variant 模式。

### 建议与恢复要区分

`expected_ident_found` 不只生成建议，还可能设置 recovered identifier。过滤条件变化时，应同时核对诊断展示和继续解析的行为，而不只是某一条 help 是否消失。

### 成本不只是集合大小

[`Parser`](compiler/rustc_parse/src/parser/mod.rs) 实现 `Clone`，诊断 snapshot 会克隆整个 parser；源码还特意对其大小做静态断言。正常解析中的收集、作用域保存，以及错误恢复中的快照复制都可能增加开销。

此次没有基准数据，不能确定具体性能回退大小。

## 12. 小结

最值得优先验证的是：

1. **`not`**：已有快照直接展示已声明变量仍被当作否定运算符。
2. **索引补 `!`**：建议条件宽泛，可见局部值可作为意图证据，但不能排除同名宏。
3. **模式表达式提取 `const`**：可靠的局部引用信息可以排除不成立的建议。
4. **复合赋值和链式比较**：更适合调整建议置信度、减少噪声，目前不支持据此自动提高 applicability。

总体上，可靠的正面证据更适合用于排除不成立的建议；不要把集合中缺名视作完整名称解析的否定结果。
