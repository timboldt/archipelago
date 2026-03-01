use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(EnumIter, Debug, Copy, Clone, PartialEq)]
pub enum Asset {
    Quarry,
    Forest,
    Field,
    Stone,
    Wood,
    Crops,
    Livestock,
    Meat,
    Fish,
    FishingBoat,
    WarShip,
    TraderShip,
    WoodenHouse,
    StoneHouse,
    Fortification,
    Children,
    Adults,
}

pub struct AssetQuantity {
    asset: Asset,
    quantity: f32,
    consumed: bool,
}

impl Asset {
    pub fn production_requirements(&self) -> Vec<AssetQuantity> {
        match self {
            Asset::Quarry => vec![],
            Asset::Forest => vec![],
            Asset::Field => vec![],
            Asset::Stone => vec![
                AssetQuantity {
                    asset: Asset::Quarry,
                    quantity: 0.1,
                    consumed: false,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 1.,
                    consumed: false,
                },
            ],
            Asset::Wood => vec![
                AssetQuantity {
                    asset: Asset::Forest,
                    quantity: 0.1,
                    consumed: false,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 1.,
                    consumed: false,
                },
            ],
            Asset::Crops => vec![
                AssetQuantity {
                    asset: Asset::Field,
                    quantity: 0.1,
                    consumed: false,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 1.,
                    consumed: false,
                },
            ],
            Asset::Livestock => vec![
                AssetQuantity {
                    asset: Asset::Crops,
                    quantity: 0.5,
                    consumed: true,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 0.05,
                    consumed: false,
                },
            ],
            Asset::Meat => vec![
                AssetQuantity {
                    asset: Asset::Livestock,
                    quantity: 0.5,
                    consumed: true,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 0.05,
                    consumed: false,
                },
            ],
            Asset::Fish => vec![
                AssetQuantity {
                    asset: Asset::FishingBoat,
                    quantity: 0.1,
                    consumed: false,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 1.,
                    consumed: false,
                },
            ],
            Asset::FishingBoat => vec![
                AssetQuantity {
                    asset: Asset::Wood,
                    quantity: 10.,
                    consumed: true,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 10.,
                    consumed: false,
                },
            ],
            Asset::WarShip => vec![
                AssetQuantity {
                    asset: Asset::Wood,
                    quantity: 1000.,
                    consumed: true,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 100.,
                    consumed: false,
                },
            ],
            Asset::TraderShip => vec![
                AssetQuantity {
                    asset: Asset::Wood,
                    quantity: 100.,
                    consumed: true,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 30.,
                    consumed: false,
                },
            ],
            Asset::WoodenHouse => vec![
                AssetQuantity {
                    asset: Asset::Wood,
                    quantity: 10.,
                    consumed: true,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 10.,
                    consumed: false,
                },
            ],
            Asset::StoneHouse => vec![
                AssetQuantity {
                    asset: Asset::Stone,
                    quantity: 10.,
                    consumed: true,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 30.,
                    consumed: false,
                },
            ],
            Asset::Fortification => vec![
                AssetQuantity {
                    asset: Asset::Stone,
                    quantity: 100.,
                    consumed: true,
                },
                AssetQuantity {
                    asset: Asset::Adults,
                    quantity: 100.,
                    consumed: false,
                },
            ],
            Asset::Children => vec![],
            Asset::Adults => vec![],
        }
    }

    pub fn 
// IDEAS:
//  Assume 1 econ unit is 1 adult for 1 season. Then, given an array of asset values, compute which one to produce.

}
