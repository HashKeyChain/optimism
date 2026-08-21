//! Derives for Base H20 storage accounting ports.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

pub(crate) fn derive_token(input: DeriveInput) -> proc_macro::TokenStream {
    expand_token(input).unwrap_or_else(syn::Error::into_compile_error).into()
}

pub(crate) fn derive_stablecoin(input: DeriveInput) -> proc_macro::TokenStream {
    expand_stablecoin(input).unwrap_or_else(syn::Error::into_compile_error).into()
}

pub(crate) fn derive_asset(input: DeriveInput) -> proc_macro::TokenStream {
    expand_asset(input).unwrap_or_else(syn::Error::into_compile_error).into()
}

fn expand_token(input: DeriveInput) -> syn::Result<TokenStream> {
    require_field(&input, "h20")?;
    let has_asset = has_field(&input, "asset");
    let name = input.ident;
    // This branch is only generated for token structs that do NOT have an `asset` field —
    // i.e. stablecoin token structs. Asset token structs always have an `asset` field and
    // take the `has_asset` branch above (which delegates to `AssetAccounting::decimals` and
    // reads per-token decimals from storage). Therefore `H20Variant::Asset` is structurally
    // unreachable in the generated code below; `H20Variant::Asset.decimals()` returning `None`
    // cannot occur here. The only `None` case from `from_address` is an unrecognized (non-H20)
    // address, which `unwrap_or(0)` handles the same way the original `map_or(0, ...)` did.
    let decimals_impl = if has_asset {
        quote! { crate::AssetAccounting::decimals(self) }
    } else {
        quote! {
            Ok(crate::H20Variant::from_address(
                ::h20_precompile_storage::ContractStorage::address(self),
            )
            .and_then(|v| v.decimals())
            .unwrap_or(0))
        }
    };

    Ok(quote! {
        impl #name<'_> {
            fn __require_policy_type(
                policy_scope: ::alloy_primitives::B256,
            ) -> ::h20_precompile_storage::Result<crate::H20PolicyType> {
                crate::H20PolicyType::from_id(policy_scope).ok_or_else(|| {
                    ::h20_precompile_storage::BasePrecompileError::revert(
                        crate::IH20::UnsupportedPolicyType { policyScope: policy_scope },
                    )
                })
            }
        }

        impl crate::TokenAccounting for #name<'_> {
            fn token_address(&self) -> ::alloy_primitives::Address {
                ::h20_precompile_storage::ContractStorage::address(self)
            }

            fn is_initialized(&self) -> ::h20_precompile_storage::Result<bool> {
                ::h20_precompile_storage::ContractStorage::is_initialized(self)
            }

            fn balance_of(
                &self,
                account: ::alloy_primitives::Address,
            ) -> ::h20_precompile_storage::Result<::alloy_primitives::U256> {
                self.h20.balance_of(account)
            }

            fn set_balance(
                &mut self,
                account: ::alloy_primitives::Address,
                balance: ::alloy_primitives::U256,
            ) -> ::h20_precompile_storage::Result<()> {
                self.h20.set_balance(account, balance)
            }

            fn allowance(
                &self,
                owner: ::alloy_primitives::Address,
                spender: ::alloy_primitives::Address,
            ) -> ::h20_precompile_storage::Result<::alloy_primitives::U256> {
                self.h20.allowance(owner, spender)
            }

            fn set_allowance(
                &mut self,
                owner: ::alloy_primitives::Address,
                spender: ::alloy_primitives::Address,
                amount: ::alloy_primitives::U256,
            ) -> ::h20_precompile_storage::Result<()> {
                self.h20.set_allowance(owner, spender, amount)
            }

            fn total_supply(
                &self,
            ) -> ::h20_precompile_storage::Result<::alloy_primitives::U256> {
                self.h20.total_supply()
            }

            fn set_total_supply(
                &mut self,
                supply: ::alloy_primitives::U256,
            ) -> ::h20_precompile_storage::Result<()> {
                self.h20.set_total_supply(supply)
            }

            fn supply_cap(&self) -> ::h20_precompile_storage::Result<::alloy_primitives::U256> {
                self.h20.supply_cap()
            }

            fn set_supply_cap(
                &mut self,
                cap: ::alloy_primitives::U256,
            ) -> ::h20_precompile_storage::Result<()> {
                self.h20.set_supply_cap(cap)
            }

            fn name(&self) -> ::h20_precompile_storage::Result<::alloc::string::String> {
                self.h20.name()
            }

            fn set_name(
                &mut self,
                name: ::alloc::string::String,
            ) -> ::h20_precompile_storage::Result<()> {
                self.h20.set_name(name)
            }

            fn symbol(&self) -> ::h20_precompile_storage::Result<::alloc::string::String> {
                self.h20.symbol()
            }

            fn set_symbol(
                &mut self,
                symbol: ::alloc::string::String,
            ) -> ::h20_precompile_storage::Result<()> {
                self.h20.set_symbol(symbol)
            }

            fn decimals(&self) -> ::h20_precompile_storage::Result<u8> {
                #decimals_impl
            }

            fn paused(&self) -> ::h20_precompile_storage::Result<::alloy_primitives::U256> {
                self.h20.paused()
            }

            fn set_paused(
                &mut self,
                vectors: ::alloy_primitives::U256,
            ) -> ::h20_precompile_storage::Result<()> {
                self.h20.set_paused(vectors)
            }

            fn nonce(
                &self,
                owner: ::alloy_primitives::Address,
            ) -> ::h20_precompile_storage::Result<::alloy_primitives::U256> {
                self.h20.nonce(owner)
            }

            fn increment_nonce(
                &mut self,
                owner: ::alloy_primitives::Address,
            ) -> ::h20_precompile_storage::Result<()> {
                let current = self.h20.nonce(owner)?;
                let next = current
                    .checked_add(::alloy_primitives::U256::ONE)
                    .ok_or_else(::h20_precompile_storage::BasePrecompileError::under_overflow)?;
                self.h20.set_nonce(owner, next)
            }

            fn contract_uri(&self) -> ::h20_precompile_storage::Result<::alloc::string::String> {
                self.h20.contract_uri()
            }

            fn set_contract_uri(
                &mut self,
                uri: ::alloc::string::String,
            ) -> ::h20_precompile_storage::Result<()> {
                self.h20.set_contract_uri(uri)
            }

            fn has_role(
                &self,
                role: ::alloy_primitives::B256,
                account: ::alloy_primitives::Address,
            ) -> ::h20_precompile_storage::Result<bool> {
                self.h20.has_role(role, account)
            }

            fn set_role(
                &mut self,
                role: ::alloy_primitives::B256,
                account: ::alloy_primitives::Address,
                enabled: bool,
            ) -> ::h20_precompile_storage::Result<()> {
                self.h20.set_role(role, account, enabled)
            }

            fn role_member_count(
                &self,
                role: ::alloy_primitives::B256,
            ) -> ::h20_precompile_storage::Result<::alloy_primitives::U256> {
                if role == crate::H20TokenRole::DefaultAdmin.id() {
                    self.h20.admin_count()
                } else {
                    Ok(::alloy_primitives::U256::ZERO)
                }
            }

            fn set_role_member_count(
                &mut self,
                role: ::alloy_primitives::B256,
                count: ::alloy_primitives::U256,
            ) -> ::h20_precompile_storage::Result<()> {
                if role == crate::H20TokenRole::DefaultAdmin.id() {
                    self.h20.set_admin_count(count)
                } else {
                    Ok(())
                }
            }

            fn role_admin(
                &self,
                role: ::alloy_primitives::B256,
            ) -> ::h20_precompile_storage::Result<::alloy_primitives::B256> {
                self.h20.role_admin(role)
            }

            fn set_role_admin(
                &mut self,
                role: ::alloy_primitives::B256,
                admin_role: ::alloy_primitives::B256,
            ) -> ::h20_precompile_storage::Result<()> {
                self.h20.set_role_admin(role, admin_role)
            }

            fn policy_id(
                &self,
                policy_scope: ::alloy_primitives::B256,
            ) -> ::h20_precompile_storage::Result<u64> {
                match Self::__require_policy_type(policy_scope)? {
                    crate::H20PolicyType::TransferSender => self.h20.transfer_sender_policy_id(),
                    crate::H20PolicyType::TransferReceiver => {
                        self.h20.transfer_receiver_policy_id()
                    }
                    crate::H20PolicyType::TransferExecutor => {
                        self.h20.transfer_executor_policy_id()
                    }
                    crate::H20PolicyType::MintReceiver => self.h20.mint_receiver_policy_id(),
                }
            }

            fn set_policy_id(
                &mut self,
                policy_scope: ::alloy_primitives::B256,
                policy_id: u64,
            ) -> ::h20_precompile_storage::Result<()> {
                match Self::__require_policy_type(policy_scope)? {
                    crate::H20PolicyType::TransferSender => {
                        self.h20.set_transfer_sender_policy_id(policy_id)
                    }
                    crate::H20PolicyType::TransferReceiver => {
                        self.h20.set_transfer_receiver_policy_id(policy_id)
                    }
                    crate::H20PolicyType::TransferExecutor => {
                        self.h20.set_transfer_executor_policy_id(policy_id)
                    }
                    crate::H20PolicyType::MintReceiver => {
                        self.h20.set_mint_receiver_policy_id(policy_id)
                    }
                }
            }

            fn emit_event(
                &mut self,
                log: ::alloy_primitives::LogData,
            ) -> ::h20_precompile_storage::Result<()> {
                self.emit_event(log)
            }
        }
    })
}

fn expand_stablecoin(input: DeriveInput) -> syn::Result<TokenStream> {
    require_field(&input, "stablecoin")?;
    let name = input.ident;
    Ok(quote! {
        impl crate::StablecoinAccounting for #name<'_> {
            fn currency(&self) -> ::h20_precompile_storage::Result<::alloc::string::String> {
                self.stablecoin.currency()
            }

            fn set_currency(
                &mut self,
                currency: ::alloc::string::String,
            ) -> ::h20_precompile_storage::Result<()> {
                self.stablecoin.set_currency(currency)
            }
        }
    })
}

fn expand_asset(input: DeriveInput) -> syn::Result<TokenStream> {
    require_field(&input, "asset")?;
    let name = input.ident;
    Ok(quote! {
        impl crate::AssetAccounting for #name<'_> {
            fn multiplier(
                &self,
            ) -> ::h20_precompile_storage::Result<::alloy_primitives::U256> {
                let multiplier = self.asset.multiplier()?;
                Ok(if multiplier.is_zero() { Self::WAD } else { multiplier })
            }

            fn set_multiplier(
                &mut self,
                multiplier: ::alloy_primitives::U256,
            ) -> ::h20_precompile_storage::Result<()> {
                self.asset.set_multiplier(multiplier)
            }

            fn extra_metadata(
                &self,
                key: &str,
            ) -> ::h20_precompile_storage::Result<::alloc::string::String> {
                ::h20_precompile_storage::Handler::read(
                    self.asset
                        .extra_metadata
                        .at(&::alloc::string::String::from(key)),
                )
            }

            fn set_extra_metadata_value(
                &mut self,
                key: &str,
                value: ::alloc::string::String,
            ) -> ::h20_precompile_storage::Result<()> {
                let key = ::alloc::string::String::from(key);
                if value.is_empty() {
                    ::h20_precompile_storage::Handler::delete(self.asset.extra_metadata.at_mut(&key))
                } else {
                    ::h20_precompile_storage::Handler::write(
                        self.asset.extra_metadata.at_mut(&key),
                        value,
                    )
                }
            }

            fn is_announcement_id_used(
                &self,
                id: &str,
            ) -> ::h20_precompile_storage::Result<bool> {
                ::h20_precompile_storage::Handler::read(
                    self.asset
                        .used_announcement_ids
                        .at(&::alloc::string::String::from(id)),
                )
            }

            fn mark_announcement_id_used(
                &mut self,
                id: &str,
            ) -> ::h20_precompile_storage::Result<()> {
                ::h20_precompile_storage::Handler::write(
                    self.asset
                        .used_announcement_ids
                        .at_mut(&::alloc::string::String::from(id)),
                    true,
                )
            }

            fn decimals(&self) -> ::h20_precompile_storage::Result<u8> {
                let stored = self.asset.decimals()?;
                Ok(if stored == 0 { Self::MIN_DECIMALS } else { stored })
            }
        }
    })
}

fn require_field(input: &DeriveInput, name: &str) -> syn::Result<()> {
    if has_field(input, name) {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(&input.ident, format!("missing `{name}` field")))
    }
}

fn has_field(input: &DeriveInput, name: &str) -> bool {
    let Data::Struct(data) = &input.data else {
        return false;
    };
    let Fields::Named(fields) = &data.fields else {
        return false;
    };
    fields.named.iter().any(|field| field.ident.as_ref().is_some_and(|ident| ident == name))
}
