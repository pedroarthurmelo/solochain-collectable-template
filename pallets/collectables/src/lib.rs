#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ExistenceRequirement},
    sp_runtime::traits::{AccountIdConversion, StaticLookup},
};
use frame_system::pallet_prelude::*;
use codec::{Encode, Decode};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::convert::TryInto;
use frame_support::pallet_prelude::BoundedVec;
use frame_support::pallet_prelude::ConstU32;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_balances::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type Currency: Currency<Self::AccountId>;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) fn generate_store)]
    #[pallet::storage_version = "1"]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub(super) type CountForKitties<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    pub(super) type Kitties<T: Config> = StorageMap<_, Blake2_128Concat, [u8; 32], Kitty<T>>;

    #[pallet::storage]
    pub(super) type KittiesOwned<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<[u8; 32], ConstU32<100>>,
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        Created(T::AccountId),
        Transferred(T::AccountId, T::AccountId, [u8; 32]),
        PriceSet(T::AccountId, [u8; 32], Option<T::Balance>),
        Sold(T::AccountId, [u8; 32], T::Balance),
    }

    #[pallet::error]
    pub enum Error<T> {
        TooManyKitties,
        TooManyOwned,
        DuplicateKitty,
        NoKitty,
        NotOwner,
        TransferToSelf,
        NotForSale,
        MaxPriceTooLow,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(10_000)]
        pub fn create_kitty(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let dna = Self::gen_dna();
            Self::mint(who, dna)?;
            Ok(())
        }

        #[pallet::weight(10_000)]
        pub fn transfer(origin: OriginFor<T>, to: <T::Lookup as StaticLookup>::Source, kitty_id: [u8; 32]) -> DispatchResult {
            let from = ensure_signed(origin)?;
            let to = T::Lookup::lookup(to)?;
            Self::do_transfer(from, to, kitty_id)?;
            Ok(())
        }

        #[pallet::weight(10_000)]
        pub fn set_price(origin: OriginFor<T>, kitty_id: [u8; 32], new_price: Option<T::Balance>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_set_price(who, kitty_id, new_price)?;
            Ok(())
        }

        #[pallet::weight(10_000)]
        pub fn buy_kitty(origin: OriginFor<T>, kitty_id: [u8; 32], price: T::Balance) -> DispatchResult {
            let buyer = ensure_signed(origin)?;
            Self::do_buy_kitty(buyer, kitty_id, price)?;
            Ok(())
        }
    }
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct Kitty<T: Config> {
    pub dna: [u8; 32],
    pub owner: T::AccountId,
    pub price: Option<T::Balance>,
}
