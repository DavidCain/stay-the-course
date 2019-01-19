extern crate rust_decimal;

use self::rust_decimal::Decimal;

#[derive(Debug, PartialEq, Eq)]
pub struct Asset {
    pub asset_class: AssetClass,
    pub name: String,
    pub value: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AssetClass {
    USBonds,
    USStocks,
    IntlBonds,
    IntlStocks,
    REIT,
    Target,
    Cash,
}

pub fn classify(fund_name: &str) -> AssetClass {
    match fund_name {
        "VTSAX" => AssetClass::USStocks,
        "VFIAX" => AssetClass::USStocks,
        "FZROX" => AssetClass::USStocks,
        "FZILX" => AssetClass::IntlStocks,
        "VTIAX" => AssetClass::IntlStocks,
        "VBTLX" => AssetClass::USBonds,
        "VGSLX" => AssetClass::REIT,
        "FZFXX" => AssetClass::Cash,   // Fidelity core position
        "VMFXX" => AssetClass::Cash,   // Vanguard settlement fund
        "VFFVX" => AssetClass::Target, // Target 2055
        "VTTSX" => AssetClass::Target, // Target 2060
        _ => panic!("Unknown fund name {:?}", fund_name),
    }
}
