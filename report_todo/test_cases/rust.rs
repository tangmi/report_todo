fn main() {
    
    let a = "";
    let a = "   ";
    let a = " // fake comment!  \" ";
    todo!("with message");
    todo!   (format!("with {}", "expression"));


    // TODOline comment
    // TODO line comment
    //TODO
    // TODO
    // TODO:line comment
    // TODO: line comment
    // TODO(#1234):line comment
    // TODO(#1234): line comment
    // TODO(#foo): line comment
    // TODO(1234):line comment
    // TODO():line comment
    // TODO(#):line comment

    todo!("\#1234: woo");
    unimplemented!();

    let a = "todo(#1234): inside a string literal";

/*  
toDO in block comment
toDOin block comment
toDO:in block comment
toDO: in block comment
toDO(#234):We are in a block comment
toDO(#235): in block comment
// hello todo line comment inside a block comment
/* */ djkflsdjfkl&  */ 


    Ok(())
}

// fn get_rest_of_line(maybe_multi_line: &str) -> &str {
//     if let Some(end_of_line) = maybe_multi_line.find("\n") {
//         &maybe_multi_line[..end_of_line]
//     } else {
//         maybe_multi_line
//     }
// }
// fn search_for_todo(input: &str) {
//     for (i, _) in input.to_ascii_lowercase().match_indices("todo") {
//         dbg!(&input[i..]);
//     }
// }
// FIXME: haha
