// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Special parameter providers (former `parameterprovider.special`: `EmptyParameterProvider`,
//! `FixedParameterProvider`, `CombinedParameterProvider`, `RangeFilterParameterProvider`).

use super::{ParamRef, ParameterProvider, SharedProvider};
use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;

/// Provides a number of empty parameters (former `EmptyParameterProvider`).
pub struct EmptyParameterProvider {
    size: usize,
}

impl EmptyParameterProvider {
    pub fn new(size: usize) -> Self {
        EmptyParameterProvider { size }
    }
}

impl ParameterProvider for EmptyParameterProvider {
    fn size(&self) -> usize {
        self.size
    }

    fn get(&self, _ctx: &Ctx, _index: usize) -> ParamRef {
        ParamRef::Empty
    }
}

/// Provides a fixed list of parameters (former `FixedParameterProvider`).
pub struct FixedParameterProvider {
    parameters: Vec<ParamRef>,
}

impl FixedParameterProvider {
    pub fn new(parameters: Vec<ParamRef>) -> Self {
        FixedParameterProvider { parameters }
    }

    /// A provider for a single parameter.
    pub fn single(parameter: ParamRef) -> Self {
        FixedParameterProvider { parameters: vec![parameter] }
    }
}

impl ParameterProvider for FixedParameterProvider {
    fn size(&self) -> usize {
        self.parameters.len()
    }

    fn get(&self, _ctx: &Ctx, index: usize) -> ParamRef {
        self.parameters.get(index).cloned().unwrap_or_default()
    }
}

/// Combines several providers into one (former `CombinedParameterProvider`).
pub struct CombinedParameterProvider {
    providers: Vec<SharedProvider>,
    overall_size: usize,
}

impl CombinedParameterProvider {
    pub fn new(providers: Vec<SharedProvider>) -> Self {
        let overall_size = providers.iter().map(|p| p.size()).sum();
        CombinedParameterProvider { providers, overall_size }
    }

    /// Find the provider and the index in it.
    fn locate(&self, index: usize) -> Option<(&SharedProvider, usize)> {
        let mut pos = index;
        for provider in &self.providers {
            let size = provider.size();
            if pos < size {
                return Some((provider, pos));
            }
            pos -= size;
        }
        None
    }
}

impl ParameterProvider for CombinedParameterProvider {
    fn size(&self) -> usize {
        self.overall_size
    }

    fn get(&self, ctx: &Ctx, index: usize) -> ParamRef {
        self.locate(index).map(|(provider, pos)| provider.get(ctx, pos)).unwrap_or_default()
    }

    fn color(&self, ctx: &Ctx, index: usize) -> Option<ColorEx> {
        self.locate(index).and_then(|(provider, pos)| provider.color(ctx, pos))
    }
}

/// Provides only a range of the parameters of another provider (former
/// `RangeFilterParameterProvider`).
pub struct RangeFilterParameterProvider {
    provider: SharedProvider,
    start_index: usize,
    length: usize,
}

impl RangeFilterParameterProvider {
    pub fn new(provider: SharedProvider, start_index: usize, length: usize) -> Self {
        RangeFilterParameterProvider { provider, start_index, length }
    }
}

impl ParameterProvider for RangeFilterParameterProvider {
    fn size(&self) -> usize {
        self.length
    }

    fn get(&self, ctx: &Ctx, index: usize) -> ParamRef {
        self.provider.get(ctx, self.start_index + index)
    }

    fn color(&self, ctx: &Ctx, index: usize) -> Option<ColorEx> {
        self.provider.color(ctx, self.start_index + index)
    }
}
