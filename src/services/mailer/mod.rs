use std::{borrow::Cow, str::FromStr, sync::Arc};

use email_address::EmailAddress;
use lettre::{address::Envelope, message::Mailbox, Address, Message, SmtpTransport, Transport};
use tokio::sync::mpsc::{self, Receiver, Sender};
use trust_dns_resolver::{config::{ResolverConfig, ResolverOpts}, Resolver};
use crate::services::smtpd::SmtpMail;

pub enum MailerMessage {
    OutgoingMail { field1: String, field2: Arc<SmtpMail> }
}
//Service that sends outgoing mails.
pub struct MailerService {
    recv_channel : mpsc::Receiver<MailerMessage>,
    send_channel : mpsc::Sender<MailerMessage>,
    dns_resolver: Resolver
}

impl MailerService {
    pub fn new() -> Self {
        let (send, recv) = mpsc::channel(512);
        MailerService { recv_channel: recv, send_channel: send, dns_resolver: Resolver::new(ResolverConfig::google(), ResolverOpts::default()).unwrap() }
    }
    pub fn get_sender_channel(&self) -> Sender<MailerMessage> {
        self.send_channel.clone()
    }

    pub async fn start_processing(&mut self) {
        while let Some(message) = self.recv_channel.recv().await {
            match message {
                MailerMessage::OutgoingMail { field1: user_addr, field2: mail } => {
                    let domain_name_rslt = EmailAddress::from_str(&user_addr);
                    if domain_name_rslt.is_err() {
                        continue;
                    }

                    let domain_name = domain_name_rslt.unwrap().domain().to_string();
                    let resp = self.dns_resolver.mx_lookup(domain_name).unwrap();
                    let address = resp.iter().next().unwrap().exchange().to_utf8();
                   
                    
                    let mailer = SmtpTransport::relay(&address).unwrap().build();
                    let to_addrs : Vec<Address> = mail.rcpt.iter().map(|f|{f.parse::<Address>().unwrap()}).collect();
                    let envelop = Envelope::new(Some(mail.from.parse::<Address>().unwrap()), to_addrs).unwrap();
                    match mailer.send_raw(&envelop, mail.message_body.as_bytes()) {
                        Ok(_) => {

                        },
                        Err(_) => {
                            
                        },
                    }

                }
            }
        }
    }
}