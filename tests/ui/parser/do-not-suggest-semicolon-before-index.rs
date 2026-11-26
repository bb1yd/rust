// Don't t suggest placing a semicolon between `indexee` and `[` if we stumble upon a comma in the
// index since it's unlikely the case that the user meant to write an array in a separate stmt.
//
// issue: <https://github.com/rust-lang/rust/issues/161147>

fn f() {
    indexee[ //~ HELP:you might have meant to call a macro
        receiver
        .method(), //~ ERROR: expected one of `.`, `?`, `]`, or an operator, found `,`
    ]
}

fn g() {
    (indexee[ //~ HELP:you might have meant to call a macro
        receiver
        .method(), //~ ERROR: expected one of `.`, `?`, `]`, or an operator, found `,`
    ])
}

fn main() {}
