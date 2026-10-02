use crate::client::{encode_component, Client, Response};
use crate::error::Error;
use crate::page::{list_page, ListParams, Page};
use crate::resources::Attributes;

/// Saved transfer recipients. Each beneficiary embeds its bank accounts (its
/// external accounts); the one flagged `default` is used when a transfer
/// names only the `beneficiary_id`. Beneficiaries can be created in the Manza
/// dashboard or through [`create`](Beneficiaries::create), and bank accounts
/// added with [`create_external_account`](Beneficiaries::create_external_account);
/// the API never updates or deletes them.
pub struct Beneficiaries<'a> {
    pub(crate) client: &'a Client,
}

impl Beneficiaries<'_> {
    /// Calls `GET /api/beneficiaries`.
    pub fn list(&self, params: ListParams) -> Result<Page, Error> {
        list_page(self.client, "api/beneficiaries", Vec::new(), &params)
    }

    /// Calls `GET /api/beneficiaries/:id`.
    pub fn get(&self, id: &str) -> Result<Response, Error> {
        self.client
            .get(&format!("api/beneficiaries/{}", encode_component(id)), &[])
    }

    /// Calls `POST /api/beneficiaries`.
    ///
    /// Attributes: `beneficiary_type` (`individual` or `business`),
    /// `person_name` or `company_name`, `email`, `phone_number`. Values must
    /// be strings (non-strings return 400). Needs the `beneficiaries:write`
    /// scope and shares the 10/min recipient-creation limit.
    pub fn create(&self, attributes: &Attributes) -> Result<Response, Error> {
        self.client.post("api/beneficiaries", Some(attributes))
    }

    /// Calls `GET /api/beneficiaries/:id/external_accounts` (paginated).
    /// Items: `{id, name, account_number, bank_identifier, currency_code,
    /// default}`.
    pub fn list_external_accounts(
        &self,
        beneficiary_id: &str,
        params: ListParams,
    ) -> Result<Page, Error> {
        list_page(
            self.client,
            &format!(
                "api/beneficiaries/{}/external_accounts",
                encode_component(beneficiary_id)
            ),
            Vec::new(),
            &params,
        )
    }

    /// Calls `GET /api/beneficiaries/:id/external_accounts/:external_account_id`.
    pub fn get_external_account(&self, beneficiary_id: &str, id: &str) -> Result<Response, Error> {
        self.client.get(
            &format!(
                "api/beneficiaries/{}/external_accounts/{}",
                encode_component(beneficiary_id),
                encode_component(id)
            ),
            &[],
        )
    }

    /// Calls `POST /api/beneficiaries/:id/external_accounts`.
    ///
    /// Required: `account_number`. `bank_identifier` is required in ZA and
    /// forbidden in MA. Optional: `name`, `country_code`, `currency_code`,
    /// `account_type` (`bank` only). Needs the `beneficiaries:write` scope.
    pub fn create_external_account(
        &self,
        beneficiary_id: &str,
        attributes: &Attributes,
    ) -> Result<Response, Error> {
        self.client.post(
            &format!(
                "api/beneficiaries/{}/external_accounts",
                encode_component(beneficiary_id)
            ),
            Some(attributes),
        )
    }
}
