use std::cmp::Eq;
use std::collections::HashMap;
use std::hash::Hash;

pub trait CheckDefault: Default {
    fn is_default(&self) -> bool;
}

impl<V> CheckDefault for Option<V> {
    fn is_default(&self) -> bool {
        self.is_none()
    }
}

pub trait MapUpdate<K, V> {
    fn update<F>(&mut self, key: K, update_func: F)
    where
        F: Sized + FnOnce(V) -> V;

    fn update_return<F, R>(&mut self, key: K, update_func: F) -> R
    where
        F: Sized + FnOnce(V) -> (V, R);
}

impl<K, V> MapUpdate<K, V> for HashMap<K, V>
where
    K: Eq + Hash,
    V: CheckDefault,
{
    fn update<F>(&mut self, key: K, update_func: F)
    where
        F: Sized + FnOnce(V) -> V,
    {
        let new_value = update_func(self.remove(&key).unwrap_or_default());
        if !new_value.is_default() {
            self.insert(key, new_value);
        }
    }

    fn update_return<F, R>(&mut self, key: K, update_func: F) -> R
    where
        F: Sized + FnOnce(V) -> (V, R),
    {
        let (new_value, retval) = update_func(self.remove(&key).unwrap_or_default());
        if !new_value.is_default() {
            self.insert(key, new_value);
        }
        retval
    }
}

#[cfg(test)]
impl CheckDefault for u32 {
    fn is_default(&self) -> bool {
        self.clone() == Default::default()
    }
}

#[test]
fn test_map_update() {
    let mut map = HashMap::new();
    let plus_one = |x: u32| x + 1;
    let sub_one = |x: u32| x - 1;
    map.update("A", plus_one);
    assert_eq!(1, map.len());
    map.update("B", plus_one);
    map.update("B", plus_one);
    assert_eq!(2, map.len());
    assert_eq!(1, map.get("A").unwrap().clone());
    assert_eq!(2, map.get("B").unwrap().clone());
    map.update("A", sub_one);
    assert_eq!(1, map.len());
    assert_eq!(false, map.contains_key("A"));
    assert_eq!(2, map.get("B").unwrap().clone());
    map.update("B", sub_one);
    assert_eq!(1, map.len());
    assert_eq!(false, map.contains_key("A"));
    assert_eq!(1, map.get("B").unwrap().clone());
}
