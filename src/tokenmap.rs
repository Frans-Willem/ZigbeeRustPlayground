use std::collections::HashMap;
use std::sync::{Arc, Weak};

/**
 * Token assigned when you insert something in the TokenMap.
 * Clone and Copy are (deliberately) not implemented, as this token
 * is something that should be passed along, stored, and finally returned, but never copied.
 */
#[derive(Debug)]
pub struct Token(u64, Weak<()>);

/**
 * A Tokenmap is a storage container, that will assign a non-copyable token that allows you to retrieve your data later.
 * The intended purpose is to save data that should be saved with a request, and can be retrieved whenever the response to the request comes in.
 */
pub struct TokenMap<T> {
    token_id: Arc<()>,
    next_token: u64,
    map: HashMap<u64, T>,
}

impl<T> TokenMap<T> {
    pub fn new() -> TokenMap<T> {
        TokenMap {
            token_id: Arc::new(()),
            next_token: 0,
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, item: T) -> Token {
        let mut token = self.next_token;
        while self.map.contains_key(&token) {
            token += 1;
        }
        self.next_token = token + 1;
        self.map.insert(token, item);
        Token(token, Arc::downgrade(&self.token_id))
    }

    pub fn remove(&mut self, token: Token) -> Option<T> {
        if !token.1.ptr_eq(&Arc::downgrade(&self.token_id)) {
            panic!("Attempt to remove from TokenMap with a Token that does not belong to this TokenMap!");
        }
        self.map.remove(&token.0)
    }
}
