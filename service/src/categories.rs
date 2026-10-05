//! Sorts till item names into categories for the leaderboard.
//!
//! Item names are free text from the school's tills ("traybake", "milkshake 1.05"), so this
//! matches on keywords. Extend the lists as new items turn up.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Pudding,
    Drink,
    Other,
}

const PUDDING: &[&str] = &[
    "traybake",
    "cupcake",
    "cheesecake",
    "cake",
    "cookie",
    "brownie",
    "muffin",
    "waffle",
    "sweet snack",
    "doughnut",
    "donut",
    "crumble",
    "sponge",
    "pudding",
    "flapjack",
    "croissant",
];

const DRINK: &[&str] = &[
    "milkshake",
    "slush",
    "water",
    "juice",
    "tetra",
    "smoothie",
    "drink",
    "hot chocolate",
];

pub fn categorise(item_name: &str) -> Category {
    let name = item_name.to_lowercase();
    if PUDDING.iter().any(|k| name.contains(k)) {
        Category::Pudding
    } else if DRINK.iter().any(|k| name.contains(k)) {
        Category::Drink
    } else {
        Category::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorises_items_seen_at_school() {
        assert_eq!(categorise("traybake"), Category::Pudding);
        assert_eq!(categorise("Cupcake"), Category::Pudding);
        assert_eq!(categorise("waffle/ sweet snack"), Category::Pudding);
        assert_eq!(categorise("milkshake 1.05"), Category::Drink);
        assert_eq!(categorise("slushies"), Category::Drink);
        assert_eq!(categorise("radnor flavoured water"), Category::Drink);
        assert_eq!(categorise("tetra large 95p"), Category::Drink);
        assert_eq!(categorise("Panini"), Category::Other);
        assert_eq!(categorise("deli meal deal"), Category::Other);
    }
}
