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
    #[pallet::storage_prefix = "CountForKitties"]
    pub type CountForKitties<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::storage_prefix = "Kitties"]
    pub type Kitties<T: Config> = StorageMap<_, Blake2_128Concat, [u8; 32], Kitty<T>>;

    #[pallet::storage]
    #[pallet::storage_prefix = "KittiesOwned"]
    pub type KittiesOwned<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<[u8; 32], ConstU32<100>>,
        ValueQuery
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        Created { owner: T::AccountId },
        Transferred { from: T::AccountId, to: T::AccountId, kitty_id: [u8; 32] },
        PriceSet { owner: T::AccountId, kitty_id: [u8; 32], price: Option<T::Balance> },
        Sold { buyer: T::AccountId, kitty_id: [u8; 32], price: T::Balance },
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
        pub fn transfer(
            origin: OriginFor<T>, 
            to: <T::Lookup as StaticLookup>::Source, 
            kitty_id: [u8; 32]
        ) -> DispatchResult {
            let from = ensure_signed(origin)?;
            let to = T::Lookup::lookup(to)?;
            Self::do_transfer(from, to, kitty_id)?;
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn set_price(
            origin: OriginFor<T>, 
            kitty_id: [u8; 32], 
            new_price: Option<T::Balance>
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_set_price(who, kitty_id, new_price)?;
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(10_000)]
        pub fn buy_kitty(
            origin: OriginFor<T>, 
            kitty_id: [u8; 32], 
            price: T::Balance
        ) -> DispatchResult {
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
            frame_system::Pallet::<T>::extrinsic_index().unwrap_or(0),
            CountForKitties::<T>::get(),
        );

        BlakeTwo256::hash_of(&unique_payload).into()
    }

    pub fn mint(owner: T::AccountId, dna: [u8; 32]) -> DispatchResult {
        // Ensure no duplicate kitty
        ensure!(!Kitties::<T>::contains_key(dna), Error::<T>::DuplicateKitty);

        // Create kitty
        let kitty = Kitty { 
            dna, 
            owner: owner.clone(), 
            price: None 
        };

        // Increment kitty count
        let current_count = CountForKitties::<T>::get();
        let new_count = current_count.checked_add(1).ok_or(Error::<T>::TooManyKitties)?;

        // Add kitty to owner's collection
        KittiesOwned::<T>::try_append(&owner, dna)
            .map_err(|_| Error::<T>::TooManyOwned)?;

        // Store kitty and update count
        Kitties::<T>::insert(dna, kitty);
        CountForKitties::<T>::set(new_count);

        // Emit event
        Self::deposit_event(Event::Created { owner });

        Ok(())
    }

    pub fn do_transfer(
        from: T::AccountId, 
        to: T::AccountId, 
        kitty_id: [u8; 32]
    ) -> DispatchResult {
        // Prevent transfer to self
        ensure!(from != to, Error::<T>::TransferToSelf);

        // Retrieve kitty
        let mut kitty = Kitties::<T>::get(kitty_id).ok_or(Error::<T>::NoKitty)?;
        
        // Ensure current owner is transferring
        ensure!(kitty.owner == from, Error::<T>::NotOwner);

        // Update kitty owner
        kitty.owner = to.clone();
        kitty.price = None;  // Reset price on transfer

        // Update ownership lists
        let mut to_owned = KittiesOwned::<T>::get(&to);
        to_owned.try_push(kitty_id).map_err(|_| Error::<T>::TooManyOwned)?;

        let mut from_owned = KittiesOwned::<T>::get(&from);
        from_owned.retain(|&x| x != kitty_id);

        // Persist changes
        Kitties::<T>::insert(kitty_id, kitty);
        KittiesOwned::<T>::insert(&to, to_owned);
        KittiesOwned::<T>::insert(&from, from_owned);

        // Emit event
        Self::deposit_event(Event::Transferred { 
            from, 
            to, 
            kitty_id 
        });

        Ok(())
    }

    pub fn do_set_price(
        caller: T::AccountId,
        kitty_id: [u8; 32],
        new_price: Option<T::Balance>
    ) -> DispatchResult {
        // Retrieve and validate kitty
        let mut kitty = Kitties::<T>::get(kitty_id).ok_or(Error::<T>::NoKitty)?;
        ensure!(kitty.owner == caller, Error::<T>::NotOwner);

        // Update price
        kitty.price = new_price;
        Kitties::<T>::insert(kitty_id, kitty);

        // Emit event
        Self::deposit_event(Event::PriceSet { 
            owner: caller, 
            kitty_id, 
            price: new_price 
        });

        Ok(())
    }

    pub fn do_buy_kitty(
        buyer: T::AccountId,
        kitty_id: [u8; 32],
        price: T::Balance
    ) -> DispatchResult {
        // Retrieve kitty
        let kitty = Kitties::<T>::get(kitty_id).ok_or(Error::<T>::NoKitty)?;
        
        // Check sale conditions
        let sale_price = kitty.price.ok_or(Error::<T>::NotForSale)?;
        ensure!(price >= sale_price, Error::<T>::MaxPriceTooLow);
        ensure!(buyer != kitty.owner, Error::<T>::TransferToSelf);

        // Perform currency transfer
        T::Currency::transfer(
            &buyer, 
            &kitty.owner, 
            sale_price, 
            ExistenceRequirement::KeepAlive
        )?;

        // Transfer kitty
        Self::do_transfer(kitty.owner.clone(), buyer.clone(), kitty_id)?;

        // Emit event
        Self::deposit_event(Event::Sold { 
            buyer, 
            kitty_id, 
            price: sale_price 
        });

        Ok(())
    }
}