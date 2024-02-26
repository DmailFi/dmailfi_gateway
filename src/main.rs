use std::{borrow::Cow, collections::HashMap, str::{from_utf8, FromStr}, sync::Arc, time::{SystemTime, UNIX_EPOCH}};

use candid::{Decode, Encode};
use clap::Parser;
use dmailfi_types::{Mail, MailHeader, Rcbytes};
use email_address::EmailAddress;
use ic_agent::export::Principal;
use serde_bytes::ByteBuf;
use services::{mailer::MailerService, smtpd::SmtpMail};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use crate::services::smtpd::SmtpServer;
    fn create_from_mail_data(mail_data : &str) -> MailHeader {
        let mut subject: Option<String> = None;
        let mut content_type: Option<String> = None;
        let mut cc : Option<Vec<String>> = None;
        let mut bcc: Option<Vec<String>> = None;

        for line in mail_data.lines() {
            if let Some(s) = line.strip_prefix("Subject: ") {
                subject = Some(s.to_string());
            }
    
            if let Some(ct) = line.strip_prefix("Content-Type: ") {
                content_type = Some(ct.to_string());
            }

            if let Some(cc_value) = line.strip_prefix("Cc: ") {
                cc = Some(cc_value.split(',').map(|s| s.trim().to_string()).collect());
            }

            if let Some(bcc_value) = line.strip_prefix("Bcc: ") {
                bcc = Some(bcc_value.split(',').map(|s| s.trim().to_string()).collect());
            }
        }

        // potential data loss coverting u128 to u64
        MailHeader { from: "".to_string(), timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64, content_type, to: vec![], subject, cc, bcc }
    }
mod services;


struct Cache<K, V> {
    map: Arc<Mutex<HashMap<K, V>>>,
}

impl<K: std::cmp::Eq + std::hash::Hash + std::marker::Send + 'static, V: std::marker::Send + 'static> Cache<K, V> {
    fn new() -> Self {
        Cache {
            map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn set(&self, key: K, value: V) {
        let mut map = self.map.lock().await;
        map.insert(key, value);
    }

    async fn get(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        let map = self.map.lock().await;
        map.get(key).cloned()
    }

    async fn remove(&self, key: &K) {
        let mut map = self.map.lock().await;
        map.remove(key);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>  {
    let mut smtp = SmtpServer::new();
    let mut mailer = MailerService::new();
    let arg = Cli::parse();
    let cache = Cache::<String, String>::new();
    let cache_map = cache.map.clone();

    let mailer_send_channel = mailer.get_sender_channel();
    mailer.start_processing().await;
    let m_x_1 = mailer_send_channel.clone();

    tokio::spawn(async move {
        let _ = smtp.start_listener_thread().await;
        
        while let Some(mail) = smtp.next().await {
            let arc_mail = Arc::new(mail);
            // let vf = arc_mail.rcpt.clone();
            // let rc = Rcbytes(Arc::new(ByteBuf::from(mail.message_body)));
            let arc_mail_1 = arc_mail.clone();
            for receipient in arc_mail.rcpt.clone() {
               
                let email_address_result = EmailAddress::from_str(receipient.as_str());
                if email_address_result.is_err() {
                    continue;
                }
                // let rx = rc.clone();
                let email_addres = email_address_result.unwrap();
                let domain_name = email_addres.domain().to_string();
                let hashmap = cache_map.lock().await;
                let canister_id_cache = hashmap.get(&domain_name.to_string());
                let mut header = create_from_mail_data(from_utf8(arc_mail_1.message_body.as_bytes()).unwrap());
                header.from = arc_mail_1.from.clone();
                let canister_mail = Mail{
                    header,
                    body: Rcbytes(Arc::new(ByteBuf::from(arc_mail.message_body.as_bytes())))
                };

                if canister_id_cache.is_some() {
                    let canister_id = Principal::from_text(canister_id_cache.unwrap()).unwrap();
                    let agent = ic_agent::Agent::builder().build().unwrap();
                    tokio::spawn(async move {
                        let _ = agent.update(&canister_id, "submit_mail").with_arg(Encode!(&canister_mail).unwrap()).call_and_wait().await;
                    });
                   
                } else {
                    let agent = ic_agent::Agent::builder().build().unwrap();
                    let registry_id = Principal::from_text(arg.registry_id.clone()).unwrap();
                    let cache_map = cache_map.clone();
                    let m_x_2 = m_x_1.clone();
                    let arc_mail_2 = arc_mail_1.clone();
                    tokio::spawn(async move {
                        let v = agent.query(&registry_id, "lookup_domain_name").with_arg(Encode!(&domain_name).unwrap()).call().await;
                        if v.is_ok() {
                            let r_canister_id_rslt = Decode!(v.unwrap().as_slice(), Result<String, ()>).unwrap();
                            if r_canister_id_rslt.is_ok() {
                                let canister_resp = r_canister_id_rslt.unwrap();
                                let r_principal_id = Principal::from_text(canister_resp.clone()).unwrap();
                            agent.update(&r_principal_id, "submit_mail").with_arg(Encode!(&canister_mail).unwrap()).call_and_wait().await;
                            cache_map.lock().await.insert(domain_name.clone(), canister_resp);
                            } else {
                                //registry does not have the domain_name associated canister id.
                                let _ = m_x_2.send(services::mailer::MailerMessage::OutgoingMail { field1: receipient.clone(), field2: arc_mail_2 }).await;
                            }
                            
                        }
                    });
                }

            };
        }
    });

    Ok(())
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    registry_id : String
}
