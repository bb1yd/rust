// Don't suggest changing `array` to a macro call if there's a local binding and
// only one argument in the index expression.


fn f(){
    let array = [];
    array[receiver.method();] //~ ERROR: expected one of `.`, `?`, `]`, or an operator, found `;`
}

fn g() {
    let array = [];
    array[
        receiver
        .method(), //~ ERROR: expected one of `.`, `?`, `]`, or an operator, found `,`
    ]
}

// suggest macro when there are multiple argument
fn a(){
    let vec = [];
    let a = vec[1,2];
    //~^ ERROR expected one of `.`, `?`, `]`, or an operator, found `,`
    //~| HELP you might have meant to call a macro
}

fn main(){}
