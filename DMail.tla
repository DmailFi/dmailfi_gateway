------------------------------- MODULE DMail -------------------------------

EXTENDS Sequences, TLC, Naturals


(*--algorithm DMail

variables 
    processedEmails = <<>>;
    emailIncomingQueue = <<>>;
    emailOutgoingQueue = <<>>;
    lockIncoming = FALSE;
    lockOutgoing = FALSE;
    Messages = {1,2,3,4,5};

define
    
end define;


fair process smtpServer \in {"s1"}
begin
    Smtp:
    while emailIncomingQueue # <<>> do
        ReadMessage:
            processedEmails := Append(processedEmails, Head(emailIncomingQueue));
            emailIncomingQueue := Tail(emailIncomingQueue);
        WriteMessage:  
            if processedEmails # <<>> then
                either
                    Canister:
                        emailOutgoingQueue := Append(emailOutgoingQueue, Head(processedEmails));
                        processedEmails := Tail(processedEmails); \*head is popped off
                or
                    Http:
                        emailOutgoingQueue := Append(emailOutgoingQueue, Head(processedEmails));
                        processedEmails := Tail(processedEmails); \*head is popped off
                end either;
            else 
                goto Smtp;
            end if;
    end while;
end process;

fair process emailIncoming = "emailIncoming"
begin
    EmailIncoming:
        with i \in Messages do \* back-pressure to avoid stackoverflow
            emailIncomingQueue := Append(emailIncomingQueue, i);
        end with;
end process;


end algorithm; *)
\* BEGIN TRANSLATION (chksum(pcal) = "9f48a837" /\ chksum(tla) = "4a37bfbe")
VARIABLES processedEmails, emailIncomingQueue, emailOutgoingQueue, 
          lockIncoming, lockOutgoing, Messages, pc

vars == << processedEmails, emailIncomingQueue, emailOutgoingQueue, 
           lockIncoming, lockOutgoing, Messages, pc >>

ProcSet == ({"s1"}) \cup {"emailIncoming"}

Init == (* Global variables *)
        /\ processedEmails = <<>>
        /\ emailIncomingQueue = <<>>
        /\ emailOutgoingQueue = <<>>
        /\ lockIncoming = FALSE
        /\ lockOutgoing = FALSE
        /\ Messages = {1,2,3,4,5}
        /\ pc = [self \in ProcSet |-> CASE self \in {"s1"} -> "Smtp"
                                        [] self = "emailIncoming" -> "EmailIncoming"]

Smtp(self) == /\ pc[self] = "Smtp"
              /\ IF emailIncomingQueue # <<>>
                    THEN /\ pc' = [pc EXCEPT ![self] = "ReadMessage"]
                    ELSE /\ pc' = [pc EXCEPT ![self] = "Done"]
              /\ UNCHANGED << processedEmails, emailIncomingQueue, 
                              emailOutgoingQueue, lockIncoming, lockOutgoing, 
                              Messages >>

ReadMessage(self) == /\ pc[self] = "ReadMessage"
                     /\ processedEmails' = Append(processedEmails, Head(emailIncomingQueue))
                     /\ emailIncomingQueue' = Tail(emailIncomingQueue)
                     /\ pc' = [pc EXCEPT ![self] = "WriteMessage"]
                     /\ UNCHANGED << emailOutgoingQueue, lockIncoming, 
                                     lockOutgoing, Messages >>

WriteMessage(self) == /\ pc[self] = "WriteMessage"
                      /\ IF processedEmails # <<>>
                            THEN /\ \/ /\ pc' = [pc EXCEPT ![self] = "Canister"]
                                    \/ /\ pc' = [pc EXCEPT ![self] = "Http"]
                            ELSE /\ pc' = [pc EXCEPT ![self] = "Smtp"]
                      /\ UNCHANGED << processedEmails, emailIncomingQueue, 
                                      emailOutgoingQueue, lockIncoming, 
                                      lockOutgoing, Messages >>

Canister(self) == /\ pc[self] = "Canister"
                  /\ emailOutgoingQueue' = Append(emailOutgoingQueue, Head(processedEmails))
                  /\ processedEmails' = Tail(processedEmails)
                  /\ pc' = [pc EXCEPT ![self] = "Smtp"]
                  /\ UNCHANGED << emailIncomingQueue, lockIncoming, 
                                  lockOutgoing, Messages >>

Http(self) == /\ pc[self] = "Http"
              /\ emailOutgoingQueue' = Append(emailOutgoingQueue, Head(processedEmails))
              /\ processedEmails' = Tail(processedEmails)
              /\ pc' = [pc EXCEPT ![self] = "Smtp"]
              /\ UNCHANGED << emailIncomingQueue, lockIncoming, lockOutgoing, 
                              Messages >>

smtpServer(self) == Smtp(self) \/ ReadMessage(self) \/ WriteMessage(self)
                       \/ Canister(self) \/ Http(self)

EmailIncoming == /\ pc["emailIncoming"] = "EmailIncoming"
                 /\ \E i \in Messages:
                      emailIncomingQueue' = Append(emailIncomingQueue, i)
                 /\ pc' = [pc EXCEPT !["emailIncoming"] = "Done"]
                 /\ UNCHANGED << processedEmails, emailOutgoingQueue, 
                                 lockIncoming, lockOutgoing, Messages >>

emailIncoming == EmailIncoming

(* Allow infinite stuttering to prevent deadlock on termination. *)
Terminating == /\ \A self \in ProcSet: pc[self] = "Done"
               /\ UNCHANGED vars

Next == emailIncoming
           \/ (\E self \in {"s1"}: smtpServer(self))
           \/ Terminating

Spec == /\ Init /\ [][Next]_vars
        /\ \A self \in {"s1"} : WF_vars(smtpServer(self))
        /\ WF_vars(emailIncoming)

Termination == <>(\A self \in ProcSet: pc[self] = "Done")

\* END TRANSLATION 

\* AllMessagesProcessed == <>[](Messages \subseteq {processedEmails})

=============================================================================
\* Modification History
\* Last modified Sun Mar 03 15:48:02 CET 2024 by lee
\* Created Thu Feb 29 10:53:23 CET 2024 by lee
