// Provide diagnostics when the user writes field names in tuple struct.(issue#144595)

struct Foo(a:u8,b:u8);
//~^ ERROR expected one of `!`, `(`, `)`, `+`, `,`, `::`, or `<`, found `:`
//~| HELP if you wanted to create a tuple struct, remove field names
//~| HELP if you wanted to create a regular struct, use curly braces

struct Bar(a:u8,u8,c:u8);
//~^ ERROR expected one of `!`, `(`, `)`, `+`, `,`, `::`, or `<`, found `:`
//~| HELP if you wanted to create a tuple struct, remove field names
//~| HELP if you wanted to create a regular struct, use curly braces

enum Quz{
    Foo(a:u8),
//~^ ERROR expected one of `!`, `(`, `)`, `+`, `,`, `::`, or `<`, found `:`
//~| HELP if you wanted to create a tuple struct, remove field names
//~| HELP if you wanted to create a regular struct, use curly braces
}
//field name diagnostics should not activate in this case.
// std::string is not a valid name.
struct Baz(std::string:String);
//~^ ERROR expected one of `!`, `(`, `)`, `+`, `,`, `::`, or `<`, found `:`
