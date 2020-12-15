table! {
    numbers (id) {
        id -> Integer,
        number_drawn -> Integer,
        draw_date -> Integer,
    }
}

table! {
    winner (id) {
        id -> Integer,
        name -> Text,
        how -> Integer,
        when -> Text,
    }
}

allow_tables_to_appear_in_same_query!(numbers, winner,);
