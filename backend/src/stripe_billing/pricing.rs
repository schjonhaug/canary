//! Canary's tiers as priced in Stripe: loading the price list at startup,
//! serving it to the frontend and mapping a subscription's price to its tier.

use crate::utils::amount::cents_to_f64;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::StripeBilling;
use crate::stripe_client_service::Price;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingInfo {
    pub tiers: Vec<FrontendTierPricing>,
    pub yearly_discount_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendTierPricing {
    pub tier: String,
    pub name: String,
    pub description: Option<String>,
    pub monthly_price: Option<FrontendPriceInfo>,
    pub yearly_price: Option<FrontendPriceInfo>,
    pub features: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendPriceInfo {
    pub price_id: String,
    pub amount: i64, // amount in cents
    pub currency: String,
    pub interval: String,
}

impl StripeBilling {
    /// Load products and prices from Stripe and build frontend pricing structure
    pub(super) async fn load_products_and_prices(&mut self) -> Result<()> {
        tracing::info!("🔍 Loading products and prices from Stripe...");

        // Fetch all products from Stripe
        let product_list = self.client.list_products(Some(100)).await?;
        let products = product_list.data.unwrap_or_default();

        tracing::info!("📦 Found {} products in Stripe", products.len());

        let mut tiers: HashMap<String, FrontendTierPricing> = HashMap::new();
        let mut monthly_amounts: Vec<i64> = Vec::new();
        let mut yearly_amounts: Vec<i64> = Vec::new();

        for product in products {
            // Skip archived or inactive products
            if product.active != Some(true) {
                continue;
            }

            // Check if product has tier metadata
            if let Some(metadata) = &product.metadata {
                if let Some(tier_str) = metadata.get("tier") {
                    let product_id = product.id.clone().unwrap_or_default();

                    tracing::info!("🎯 Found product {} for tier {}", product_id, tier_str);

                    // Get all prices for this product
                    let price_list = self.client.list_prices(Some(100), Some(product_id)).await?;
                    let prices = price_list.data.unwrap_or_default();

                    // Initialize tier pricing if not exists
                    let tier_pricing =
                        tiers
                            .entry(tier_str.clone())
                            .or_insert_with(|| FrontendTierPricing {
                                tier: tier_str.clone(),
                                name: StripeBilling::get_tier_display_name(tier_str),
                                description: StripeBilling::get_tier_description(tier_str),
                                monthly_price: None,
                                yearly_price: None,
                                features: Self::get_tier_features(tier_str),
                            });

                    // Process all active prices for this product
                    for price in prices {
                        // Skip archived prices
                        if price.active != Some(true) {
                            continue;
                        }

                        apply_recurring_price(
                            tier_pricing,
                            tier_str,
                            &price,
                            &mut monthly_amounts,
                            &mut yearly_amounts,
                        );
                    }
                }
            }
        }

        // Calculate discount percentage from actual price differences (now that yearly prices have discount built-in)
        let discount_percent = if !monthly_amounts.is_empty() && !yearly_amounts.is_empty() {
            average_yearly_discount(&tiers)
        } else {
            None
        };

        // Store the collected pricing data
        self.cached_pricing = PricingInfo {
            tiers: tiers.into_values().collect(),
            yearly_discount_percent: discount_percent,
        };

        // Sort tiers by display order (Personal -> Team)
        self.cached_pricing
            .tiers
            .sort_by_key(|tier| tier_display_order(&tier.tier));

        tracing::info!(
            "✅ Loaded {} pricing tiers with {}% yearly discount",
            self.cached_pricing.tiers.len(),
            discount_percent.unwrap_or(0.0)
        );

        Ok(())
    }

    fn get_tier_display_name(tier: &str) -> String {
        match tier.to_lowercase().as_str() {
            "personal" => "Personal".to_string(),
            "team" => "Team".to_string(),
            _ => tier.to_string(),
        }
    }

    fn get_tier_description(tier: &str) -> Option<String> {
        match tier.to_lowercase().as_str() {
            "personal" => Some("For individual Bitcoin holders".to_string()),
            "team" => Some("For Uncle Jims & family guardians".to_string()),
            _ => None,
        }
    }

    fn get_tier_features(tier: &str) -> HashMap<String, String> {
        let mut features = HashMap::new();

        // All features available on all tiers - consistent with frontend
        features.insert("trial".to_string(), "30-day free trial".to_string());
        features.insert("email".to_string(), "Email notifications".to_string());
        features.insert("sms".to_string(), "SMS notifications".to_string());
        features.insert("push".to_string(), "Push notifications".to_string());
        features.insert(
            "analysis".to_string(),
            "Transaction analysis (RBF/CPFP coming soon)".to_string(),
        );

        // Tier-specific capacity limits
        match tier.to_lowercase().as_str() {
            "personal" => {
                features.insert("wallets".to_string(), "1 wallet".to_string());
                features.insert("contacts".to_string(), "1 contact".to_string());
                features.insert("sync".to_string(), "10 minute sync time".to_string());
            }
            "team" => {
                features.insert("wallets".to_string(), "5 wallets".to_string());
                features.insert("contacts".to_string(), "5 contacts per wallet".to_string());
                features.insert("sync".to_string(), "2 minute sync time".to_string());
            }
            _ => {}
        }

        features
    }

    pub fn get_pricing_for_frontend(&self) -> PricingInfo {
        self.cached_pricing.clone()
    }

    // Helper method to determine tier from subscription items (products/prices)
    pub(super) fn determine_tier_from_subscription_items(
        &self,
        subscription: &serde_json::Value,
    ) -> String {
        if let Some(items) = subscription.get("items") {
            if let Some(data) = items.get("data") {
                if let Some(items_array) = data.as_array() {
                    for item in items_array {
                        if let Some(price) = item.get("price") {
                            if let Some(product_id) = price.get("product").and_then(|p| p.as_str())
                            {
                                // Check our cached pricing to find the tier for this product
                                for tier_info in &self.cached_pricing.tiers {
                                    if let Some(monthly_price) = &tier_info.monthly_price {
                                        // Get product from our pricing cache (we need to check against the product)
                                        // For now, use the price_id to determine tier
                                        if let Some(price_id) =
                                            price.get("id").and_then(|id| id.as_str())
                                        {
                                            if monthly_price.price_id == price_id {
                                                tracing::info!(
                                                    "🎯 Determined tier from price_id {}: {}",
                                                    price_id,
                                                    tier_info.tier
                                                );
                                                return tier_info.tier.clone();
                                            }
                                        }
                                        // Also check yearly price
                                        if let Some(yearly_price) = &tier_info.yearly_price {
                                            if let Some(price_id) =
                                                price.get("id").and_then(|id| id.as_str())
                                            {
                                                if yearly_price.price_id == price_id {
                                                    tracing::info!("🎯 Determined tier from yearly price_id {}: {}", price_id, tier_info.tier);
                                                    return tier_info.tier.clone();
                                                }
                                            }
                                        }
                                    }
                                }
                                tracing::warn!(
                                    "❓ Could not determine tier for product_id: {}",
                                    product_id
                                );
                            }
                        }
                    }
                }
            }
        }

        // Fallback to metadata-based tier detection
        if let Some(tier) = subscription
            .get("metadata")
            .and_then(|m| m.get("tier"))
            .and_then(|t| t.as_str())
        {
            tracing::info!("📋 Using tier from metadata: {}", tier);
            return tier.to_lowercase();
        }

        tracing::warn!("Could not determine subscription tier; defaulting to personal");
        "personal".to_string()
    }
}

/// Records an active Stripe price on its tier when it is a monthly or yearly
/// price cheaper than the one already found, and collects its amount.
fn apply_recurring_price(
    tier_pricing: &mut FrontendTierPricing,
    tier_str: &str,
    price: &Price,
    monthly_amounts: &mut Vec<i64>,
    yearly_amounts: &mut Vec<i64>,
) {
    let Some(recurring) = &price.recurring else {
        return;
    };
    let interval = recurring.interval.as_deref().unwrap_or("");
    let interval_count = recurring.interval_count.unwrap_or(0);
    let price_id = price.id.clone().unwrap_or_default();
    let amount = price.unit_amount.unwrap_or(0);
    let currency = price.currency.as_deref().unwrap_or("USD").to_uppercase();

    if interval == "month" && interval_count == 1 {
        // Prefer the most recent or lowest monthly price
        if tier_pricing
            .monthly_price
            .as_ref()
            .is_none_or(|current| amount < current.amount)
        {
            tier_pricing.monthly_price = Some(FrontendPriceInfo {
                price_id: price_id.clone(),
                amount,
                currency: currency.clone(),
                interval: "month".to_string(),
            });
            tracing::info!(
                "💰 Found monthly price for {}: ${:.2}/{} ({})",
                tier_str,
                cents_to_f64(amount) / 100.0,
                currency,
                price_id
            );
        }
        monthly_amounts.push(amount);
    } else if interval == "year" && interval_count == 1 {
        // Prefer the lowest yearly price (discounted price)
        if tier_pricing
            .yearly_price
            .as_ref()
            .is_none_or(|current| amount < current.amount)
        {
            tier_pricing.yearly_price = Some(FrontendPriceInfo {
                price_id: price_id.clone(),
                amount,
                currency: currency.clone(),
                interval: "year".to_string(),
            });
            tracing::info!(
                "💰 Found yearly price for {}: ${:.2}/{} ({})",
                tier_str,
                cents_to_f64(amount) / 100.0,
                currency,
                price_id
            );
        }
        yearly_amounts.push(amount);
    }
}

/// The average yearly discount, in whole percent, over the tiers whose yearly
/// price is below twelve monthly payments; `None` when no tier has one.
fn average_yearly_discount(tiers: &HashMap<String, FrontendTierPricing>) -> Option<f64> {
    let mut total_discount = 0.0;
    let mut count = 0;

    // Calculate discount for each tier by comparing monthly*12 vs yearly price
    for tier_pricing in tiers.values() {
        if let (Some(monthly), Some(yearly)) =
            (&tier_pricing.monthly_price, &tier_pricing.yearly_price)
        {
            let monthly_total = cents_to_f64(monthly.amount * 12);
            let yearly_price = cents_to_f64(yearly.amount);

            if yearly_price < monthly_total {
                let discount = (monthly_total - yearly_price) / monthly_total * 100.0;
                total_discount += discount;
                count += 1;
                tracing::info!(
                    "📊 {} tier: {}% yearly discount (${:.2}/year vs ${:.2}/year)",
                    tier_pricing.tier,
                    discount,
                    yearly_price / 100.0,
                    monthly_total / 100.0
                );
            }
        }
    }

    if count > 0 {
        Some((total_discount / f64::from(count)).round())
    } else {
        None
    }
}

/// Where a tier is listed on the pricing page (Personal before Team).
fn tier_display_order(tier: &str) -> u8 {
    match tier.to_lowercase().as_str() {
        "personal" => 1,
        "team" => 2,
        _ => 99,
    }
}
