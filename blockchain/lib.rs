#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod blockchain {
    // Импорты
    use ink::prelude::string::String;
    use ink::storage::Mapping;

    // Прямые импорты кодеков
    use scale::{Decode, Encode};
    use scale_info::TypeInfo;

    // --- ИСПРАВЛЕНИЕ ЗДЕСЬ ---
    // Добавляем StorageLayout в derive для feature = "std"
    // Это лечит ошибку со второго скриншота!
    #[derive(Decode, Encode, Clone, Debug, PartialEq)]
    #[cfg_attr(feature = "std", derive(TypeInfo, ink::storage::traits::StorageLayout))]
    pub struct Profile {
        pub username: String,
        pub bio: String,
    }

    #[derive(Decode, Encode, Debug, PartialEq, Eq)]
    #[cfg_attr(feature = "std", derive(TypeInfo))]
    pub enum Error {
        ProfileAlreadyExists,
        ProfileNotFound,
    }

    // --- ХРАНИЛИЩЕ ---
    #[ink(storage)]
    pub struct Blockchain {
        profiles: Mapping<AccountId, Profile>,
    }

    // --- ЛОГИКА ---
    impl Blockchain {
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                profiles: Mapping::default(),
            }
        }

        #[ink(message)]
        pub fn create_profile(&mut self, username: String, bio: String) -> Result<(), Error> {
            let caller = self.env().caller();

            if self.profiles.contains(caller) {
                return Err(Error::ProfileAlreadyExists);
            }

            let profile = Profile { username, bio };
            self.profiles.insert(caller, &profile);

            Ok(())
        }

        #[ink(message)]
        pub fn get_profile(&self, user: AccountId) -> Result<Profile, Error> {
            self.profiles.get(user).ok_or(Error::ProfileNotFound)
        }

        #[ink(message)]
        pub fn get_my_profile(&self) -> Result<Profile, Error> {
            let caller = self.env().caller();
            self.get_profile(caller)
        }
    }

    // --- ТЕСТЫ ---
    #[cfg(test)]
    mod tests {
        use super::*;

        #[ink::test]
        fn create_profile_works() {
            let mut contract = Blockchain::new();
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();

            assert_eq!(contract.create_profile("Alice".into(), "Hi".into()), Ok(()));

            let profile = contract.get_profile(accounts.alice).unwrap();
            assert_eq!(profile.username, "Alice");
        }
    }
}
