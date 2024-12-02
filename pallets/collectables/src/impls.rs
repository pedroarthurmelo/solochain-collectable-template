#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ExistenceRequirement},
    sp_runtime::traits::{AccountIdConversion, StaticLookup},
};
use frame_system::pallet_prelude::*;
use codec::{Encode, Decode};
use scale_info::TypeInfo;
use sp_runtime::{RuntimeDebug, traits::Zero};
use sp_std::convert::TryInto;
use frame_support::pallet_prelude::{BoundedVec, ConstU32};

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
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn create_kitty(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let dna = Self::gen_dna();
            Self::mint(who, dna)?;
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn transfer(origin: OriginFor<T>, to: <T::Lookup as StaticLookup>::Source, kitty_id: [u8; 32]) -> DispatchResult {
            let from = ensure_signed(origin)?;
            let to = T::Lookup::lookup(to)?;
            Self::do_transfer(from, to, kitty_id)?;
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn set_price(origin: OriginFor<T>, kitty_id: [u8; 32], new_price: Option<T::Balance>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_set_price(who, kitty_id, new_price)?;
            Ok(())
        }

        #[pallet::call_index(3)]
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

impl<T: Config> Pallet<T> {
    pub fn gen_dna() -> [u8; 32] {
        let unique_payload = (
            frame_system::Pallet::<T>::parent_hash(),
            frame_system::Pallet::<T>::block_number(),
            frame_system::Pallet::<T>::extrinsic_index(),
            CountForKitties::<T>::get(),
        );

        BlakeTwo256::hash_of(&unique_payload).into()
    }

    pub fn mint(owner: T::AccountId, dna: [u8; 32]) -> DispatchResult {
        let kitty = Kitty { dna, owner: owner.clone(), price: None };
        ensure!(!Kitties::<T>::contains_key(dna), Error::<T>::DuplicateKitty);

        let current_count = CountForKitties::<T>::get();
        let new_count = current_count.checked_add(1).ok_or(Error::<T>::TooManyKitties)?;

        KittiesOwned::<T>::try_append(&owner, dna).map_err(|_| Error::<T>::TooManyOwned)?;
        Kitties::<T>::insert(dna, kitty);
        CountForKitties::<T>::set(new_count);

        Self::deposit_event(Event::<T>::Created(owner));
        Ok(())
    }

    pub fn do_transfer(from: T::AccountId, to: T::AccountId, kitty_id: [u8; 32]) -> DispatchResult {
        ensure!(from != to, Error::<T>::TransferToSelf);
        let mut kitty = Kitties::<T>::get(kitty_id).ok_or(Error::<T>::NoKitty)?;
        ensure!(kitty.owner == from, Error::<T>::NotOwner);
        kitty.owner = to.clone();

        let mut to_owned = KittiesOwned::<T>::get(&to);
        to_owned.try_push(kitty_id).map_err(|_| Error::<T>::TooManyOwned)?;
        let mut from_owned = KittiesOwned::<T>::get(&from);
        if let Some(ind) = from_owned.iter().position(|&id| id == kitty_id) {
            from_owned.swap_remove(ind);
        } else {
            return Err(Error::<T>::NoKitty.into());
        }

        Kitties::<T>::insert(kitty_id, kitty);
        KittiesOwned::<T>::insert(&to, to_owned);
        KittiesOwned::<T>::insert(&from, from_owned);

        Self::deposit_event(Event::<T>::Transferred(from, to, kitty_id));
        Ok(())
    }

    pub fn do_set_price(
        caller: T::AccountId,
        kitty_id: [u8; 32],
        new_price: Option<T::Balance>,
    ) -> DispatchResult {
        let mut kitty = Kitties::<T>::get(kitty_id).ok_or(Error::<T>::NoKitty)?;
        ensure!(kitty.owner == caller, Error::<T>::NotOwner);
        kitty.price = new_price;
        Kitties::<T>::insert(kitty_id, kitty);

        Self::deposit_event(Event::<T>::PriceSet(caller, kitty_id, new_price));
        Ok(())
    }

    pub fn do_buy_kitty(
        buyer: T::AccountId,
        kitty_id: [u8; 32],
        price: T::Balance,
    ) -> DispatchResult {
        let kitty = Kitties::<T>::get(kitty_id).ok_or(Error::<T>::NoKitty)?;
        let real_price = kitty.price.ok_or(Error::<T>::NotForSale)?;
        ensure!(price >= real_price, Error::<T>::MaxPriceTooLow);

        T::Currency::transfer(&buyer, &kitty.owner, real_price, ExistenceRequirement::KeepAlive)?;
        Self::do_transfer(kitty.owner.clone(), buyer.clone(), kitty_id)?;

        Self::deposit_event(Event::<T>::Sold(buyer, kitty_id, real_price));
        Ok(())
    }
}