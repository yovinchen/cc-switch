//! OneOrMany 容器
//!
//! 借鉴 Rig 项目的类型安全设计，提供保证至少有一个元素的容器。
//! 用于替代 `Vec<T>`，在编译时消除空数组问题。

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::ser::{SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

/// 包含一个或多个元素的容器
///
/// 如果是单个元素，`first` 包含它，`rest` 为空。
/// 如果是多个元素，`first` 包含第一个，`rest` 包含其余。
///
/// **重要**: 此结构无法创建空数组。只能通过 `OneOrMany::one()` 或 `OneOrMany::many()` 创建。
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct OneOrMany<T> {
    /// 第一个元素（保证存在）
    first: T,
    /// 其余元素
    rest: Vec<T>,
}

/// 尝试从空数组创建 OneOrMany 时的错误
#[derive(Debug, thiserror::Error)]
#[error("Cannot create OneOrMany with an empty vector.")]
pub struct EmptyListError;

impl<T: Clone> OneOrMany<T> {
    /// 获取第一个元素（克隆）
    pub fn first(&self) -> T {
        self.first.clone()
    }

    /// 获取第一个元素的引用
    pub fn first_ref(&self) -> &T {
        &self.first
    }

    /// 获取第一个元素的可变引用
    pub fn first_mut(&mut self) -> &mut T {
        &mut self.first
    }

    /// 获取最后一个元素（克隆）
    pub fn last(&self) -> T {
        self.rest
            .last()
            .cloned()
            .unwrap_or_else(|| self.first.clone())
    }

    /// 获取最后一个元素的引用
    pub fn last_ref(&self) -> &T {
        self.rest.last().unwrap_or(&self.first)
    }

    /// 获取最后一个元素的可变引用
    pub fn last_mut(&mut self) -> &mut T {
        self.rest.last_mut().unwrap_or(&mut self.first)
    }

    /// 获取除第一个外的所有元素
    pub fn rest(&self) -> Vec<T> {
        self.rest.clone()
    }

    /// 添加一个元素到末尾
    pub fn push(&mut self, item: T) {
        self.rest.push(item);
    }

    /// 在指定位置插入元素
    pub fn insert(&mut self, index: usize, item: T) {
        if index == 0 {
            let old_first = std::mem::replace(&mut self.first, item);
            self.rest.insert(0, old_first);
        } else {
            self.rest.insert(index - 1, item);
        }
    }

    /// 获取元素总数
    pub fn len(&self) -> usize {
        1 + self.rest.len()
    }

    /// 是否为空（总是返回 false，因为至少有一个元素）
    pub fn is_empty(&self) -> bool {
        false
    }

    /// 创建只包含一个元素的 OneOrMany
    pub fn one(item: T) -> Self {
        OneOrMany {
            first: item,
            rest: vec![],
        }
    }

    /// 从迭代器创建 OneOrMany
    ///
    /// 如果迭代器为空，返回 `EmptyListError`
    pub fn many<I>(items: I) -> Result<Self, EmptyListError>
    where
        I: IntoIterator<Item = T>,
    {
        let mut iter = items.into_iter();
        Ok(OneOrMany {
            first: match iter.next() {
                Some(item) => item,
                None => return Err(EmptyListError),
            },
            rest: iter.collect(),
        })
    }

    /// 合并多个 OneOrMany 为一个
    pub fn merge<I>(one_or_many_items: I) -> Result<Self, EmptyListError>
    where
        I: IntoIterator<Item = OneOrMany<T>>,
    {
        let items = one_or_many_items
            .into_iter()
            .flat_map(|one_or_many| one_or_many.into_iter())
            .collect::<Vec<_>>();

        OneOrMany::many(items)
    }

    /// 映射函数，转换每个元素
    ///
    /// 由于 OneOrMany 至少有一个元素，使用 `.collect::<Vec<_>>()` 后
    /// 再 `OneOrMany::many()` 是多余的。此方法直接构造结果。
    pub fn map<U, F: FnMut(T) -> U>(self, mut op: F) -> OneOrMany<U> {
        OneOrMany {
            first: op(self.first),
            rest: self.rest.into_iter().map(op).collect(),
        }
    }

    /// 可失败的映射函数
    pub fn try_map<U, E, F>(self, mut op: F) -> Result<OneOrMany<U>, E>
    where
        F: FnMut(T) -> Result<U, E>,
    {
        Ok(OneOrMany {
            first: op(self.first)?,
            rest: self
                .rest
                .into_iter()
                .map(op)
                .collect::<Result<Vec<_>, E>>()?,
        })
    }

    /// 获取迭代器
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            first: Some(&self.first),
            rest: self.rest.iter(),
        }
    }

    /// 获取可变迭代器
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            first: Some(&mut self.first),
            rest: self.rest.iter_mut(),
        }
    }

    /// 转换为 Vec
    pub fn into_vec(self) -> Vec<T> {
        let mut v = vec![self.first];
        v.extend(self.rest);
        v
    }
}

// ================================================================
// Iterator 实现
// ================================================================

/// `OneOrMany::iter()` 返回的迭代器
pub struct Iter<'a, T> {
    first: Option<&'a T>,
    rest: std::slice::Iter<'a, T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(first) = self.first.take() {
            Some(first)
        } else {
            self.rest.next()
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let first = if self.first.is_some() { 1 } else { 0 };
        let max = self.rest.size_hint().1.unwrap_or(0) + first;
        if max > 0 {
            (1, Some(max))
        } else {
            (0, Some(0))
        }
    }
}

/// `OneOrMany::into_iter()` 返回的迭代器
pub struct IntoIter<T> {
    first: Option<T>,
    rest: std::vec::IntoIter<T>,
}

impl<T> IntoIterator for OneOrMany<T>
where
    T: Clone,
{
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            first: Some(self.first),
            rest: self.rest.into_iter(),
        }
    }
}

impl<T> Iterator for IntoIter<T>
where
    T: Clone,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.first.take() {
            Some(first) => Some(first),
            _ => self.rest.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let first = if self.first.is_some() { 1 } else { 0 };
        let max = self.rest.size_hint().1.unwrap_or(0) + first;
        if max > 0 {
            (1, Some(max))
        } else {
            (0, Some(0))
        }
    }
}

/// `OneOrMany::iter_mut()` 返回的迭代器
pub struct IterMut<'a, T> {
    first: Option<&'a mut T>,
    rest: std::slice::IterMut<'a, T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(first) = self.first.take() {
            Some(first)
        } else {
            self.rest.next()
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let first = if self.first.is_some() { 1 } else { 0 };
        let max = self.rest.size_hint().1.unwrap_or(0) + first;
        if max > 0 {
            (1, Some(max))
        } else {
            (0, Some(0))
        }
    }
}

// ================================================================
// Serde 实现
// ================================================================

/// 序列化为 JSON 数组
impl<T> Serialize for OneOrMany<T>
where
    T: Serialize + Clone,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for e in self.iter() {
            seq.serialize_element(e)?;
        }
        seq.end()
    }
}

/// 从 JSON 数组反序列化
///
/// 同时支持从单个值反序列化（使用 `OneOrMany::one`）
impl<'de, T> Deserialize<'de> for OneOrMany<T>
where
    T: Deserialize<'de> + Clone,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OneOrManyVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for OneOrManyVisitor<T>
        where
            T: Deserialize<'de> + Clone,
        {
            type Value = OneOrMany<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a sequence of at least one element")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let first = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;

                let mut rest = Vec::new();
                while let Some(value) = seq.next_element()? {
                    rest.push(value);
                }

                Ok(OneOrMany { first, rest })
            }
        }

        deserializer.deserialize_any(OneOrManyVisitor(PhantomData))
    }
}

/// 特殊反序列化函数：支持字符串或数组
///
/// 用法:
/// ```ignore
/// #[derive(Deserialize)]
/// struct MyStruct {
///     #[serde(deserialize_with = "string_or_one_or_many")]
///     field: OneOrMany<String>,
/// }
/// ```
pub fn string_or_one_or_many<'de, T, D>(deserializer: D) -> Result<OneOrMany<T>, D::Error>
where
    T: Deserialize<'de> + FromStr<Err = Infallible> + Clone,
    D: Deserializer<'de>,
{
    struct StringOrOneOrMany<T>(PhantomData<fn() -> T>);

    impl<'de, T> Visitor<'de> for StringOrOneOrMany<T>
    where
        T: Deserialize<'de> + FromStr<Err = Infallible> + Clone,
    {
        type Value = OneOrMany<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or sequence")
        }

        fn visit_str<E>(self, value: &str) -> Result<OneOrMany<T>, E>
        where
            E: de::Error,
        {
            let item = FromStr::from_str(value).map_err(de::Error::custom)?;
            Ok(OneOrMany::one(item))
        }

        fn visit_seq<A>(self, seq: A) -> Result<OneOrMany<T>, A::Error>
        where
            A: SeqAccess<'de>,
        {
            Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))
        }

        fn visit_map<M>(self, map: M) -> Result<OneOrMany<T>, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let item = Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(OneOrMany::one(item))
        }
    }

    deserializer.deserialize_any(StringOrOneOrMany(PhantomData))
}

/// 可选的字符串或数组反序列化
pub fn string_or_option_one_or_many<'de, T, D>(
    deserializer: D,
) -> Result<Option<OneOrMany<T>>, D::Error>
where
    T: Deserialize<'de> + FromStr<Err = Infallible> + Clone,
    D: Deserializer<'de>,
{
    struct StringOrOptionOneOrMany<T>(PhantomData<fn() -> T>);

    impl<'de, T> Visitor<'de> for StringOrOptionOneOrMany<T>
    where
        T: Deserialize<'de> + FromStr<Err = Infallible> + Clone,
    {
        type Value = Option<OneOrMany<T>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("null, a string, or a sequence")
        }

        fn visit_none<E>(self) -> Result<Option<OneOrMany<T>>, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Option<OneOrMany<T>>, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Option<OneOrMany<T>>, D::Error>
        where
            D: Deserializer<'de>,
        {
            string_or_one_or_many(deserializer).map(Some)
        }
    }

    deserializer.deserialize_option(StringOrOptionOneOrMany(PhantomData))
}

// ================================================================
// From 实现
// ================================================================

impl<T: Clone> From<T> for OneOrMany<T> {
    fn from(item: T) -> Self {
        OneOrMany::one(item)
    }
}

impl<T: Clone> TryFrom<Vec<T>> for OneOrMany<T> {
    type Error = EmptyListError;

    fn try_from(vec: Vec<T>) -> Result<Self, Self::Error> {
        OneOrMany::many(vec)
    }
}

impl<T: Clone> From<OneOrMany<T>> for Vec<T> {
    fn from(one_or_many: OneOrMany<T>) -> Self {
        one_or_many.into_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_one() {
        let one = OneOrMany::one("hello".to_string());
        assert_eq!(one.len(), 1);
        assert_eq!(one.first(), "hello");
        assert_eq!(one.last(), "hello");
        assert!(!one.is_empty());
    }

    #[test]
    fn test_many() {
        let many = OneOrMany::many(vec!["a".to_string(), "b".to_string(), "c".to_string()]).unwrap();
        assert_eq!(many.len(), 3);
        assert_eq!(many.first(), "a");
        assert_eq!(many.last(), "c");
    }

    #[test]
    fn test_many_empty_error() {
        let result = OneOrMany::<String>::many(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_push() {
        let mut one = OneOrMany::one("a".to_string());
        one.push("b".to_string());
        assert_eq!(one.len(), 2);
        assert_eq!(one.last(), "b");
    }

    #[test]
    fn test_insert() {
        let mut many = OneOrMany::many(vec!["a".to_string(), "c".to_string()]).unwrap();
        many.insert(1, "b".to_string());
        assert_eq!(many.len(), 3);
        let v: Vec<_> = many.iter().collect();
        assert_eq!(v, vec![&"a".to_string(), &"b".to_string(), &"c".to_string()]);
    }

    #[test]
    fn test_insert_at_zero() {
        let mut one = OneOrMany::one("b".to_string());
        one.insert(0, "a".to_string());
        assert_eq!(one.first(), "a");
        assert_eq!(one.len(), 2);
    }

    #[test]
    fn test_map() {
        let one = OneOrMany::one(1);
        let mapped = one.map(|x| x * 2);
        assert_eq!(mapped.first(), 2);
    }

    #[test]
    fn test_try_map() {
        let one = OneOrMany::one("42".to_string());
        let result: Result<OneOrMany<i32>, _> = one.try_map(|s| s.parse::<i32>());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().first(), 42);
    }

    #[test]
    fn test_try_map_error() {
        let one = OneOrMany::one("not_a_number".to_string());
        let result: Result<OneOrMany<i32>, _> = one.try_map(|s| s.parse::<i32>());
        assert!(result.is_err());
    }

    #[test]
    fn test_iter() {
        let many = OneOrMany::many(vec![1, 2, 3]).unwrap();
        let sum: i32 = many.iter().sum();
        assert_eq!(sum, 6);
    }

    #[test]
    fn test_into_iter() {
        let many = OneOrMany::many(vec![1, 2, 3]).unwrap();
        let v: Vec<_> = many.into_iter().collect();
        assert_eq!(v, vec![1, 2, 3]);
    }

    #[test]
    fn test_iter_mut() {
        let mut many = OneOrMany::many(vec![1, 2, 3]).unwrap();
        for x in many.iter_mut() {
            *x *= 2;
        }
        assert_eq!(many.first(), 2);
        assert_eq!(many.last(), 6);
    }

    #[test]
    fn test_merge() {
        let a = OneOrMany::many(vec![1, 2]).unwrap();
        let b = OneOrMany::one(3);
        let merged = OneOrMany::merge(vec![a, b]).unwrap();
        assert_eq!(merged.len(), 3);
        let v: Vec<_> = merged.into_iter().collect();
        assert_eq!(v, vec![1, 2, 3]);
    }

    #[test]
    fn test_serialize() {
        let one = OneOrMany::one("hello".to_string());
        let json = serde_json::to_string(&one).unwrap();
        assert_eq!(json, r#"["hello"]"#);

        let many = OneOrMany::many(vec!["a".to_string(), "b".to_string()]).unwrap();
        let json = serde_json::to_string(&many).unwrap();
        assert_eq!(json, r#"["a","b"]"#);
    }

    #[test]
    fn test_deserialize() {
        let json = json!(["hello", "world"]);
        let one_or_many: OneOrMany<String> = serde_json::from_value(json).unwrap();
        assert_eq!(one_or_many.len(), 2);
        assert_eq!(one_or_many.first(), "hello");
    }

    #[test]
    fn test_deserialize_single() {
        let json = json!(["only"]);
        let one_or_many: OneOrMany<String> = serde_json::from_value(json).unwrap();
        assert_eq!(one_or_many.len(), 1);
        assert_eq!(one_or_many.first(), "only");
    }

    #[test]
    fn test_into_vec() {
        let many = OneOrMany::many(vec![1, 2, 3]).unwrap();
        let v = many.into_vec();
        assert_eq!(v, vec![1, 2, 3]);
    }

    #[test]
    fn test_from_single() {
        let one: OneOrMany<i32> = 42.into();
        assert_eq!(one.first(), 42);
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn test_try_from_vec() {
        let v = vec![1, 2, 3];
        let one_or_many: OneOrMany<i32> = v.try_into().unwrap();
        assert_eq!(one_or_many.len(), 3);
    }

    #[test]
    fn test_size_hint() {
        let many = OneOrMany::many(vec![1, 2, 3]).unwrap();
        let hint = many.iter().size_hint();
        assert_eq!(hint.0, 1);
        assert_eq!(hint.1, Some(3));
    }
}
