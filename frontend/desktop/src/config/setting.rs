pub trait Resolvable {
    type Resolved;
    type Set;

    fn get(&self) -> &Self::Resolved;
    fn set(&mut self, value: Self::Set);
}

pub trait Defaultable: Resolvable {
    fn set_default(&mut self);
}

pub struct NonOverridable<T: Clone> {
    value: T,
    default: T,
}

impl<T: Clone> NonOverridable<T> {
    pub(super) fn new(value: T, default: T) -> Self {
        NonOverridable { value, default }
    }

    pub fn update(&mut self, f: impl FnOnce(&mut T)) {
        f(&mut self.value);
    }

    pub fn default(&self) -> &T {
        &self.default
    }
}

impl<T: Clone> Resolvable for NonOverridable<T> {
    type Resolved = T;
    type Set = T;

    fn get(&self) -> &Self::Resolved {
        &self.value
    }

    fn set(&mut self, value: Self::Set) {
        self.value = value;
    }
}

impl<T: Clone> Defaultable for NonOverridable<T> {
    fn set_default(&mut self) {
        self.value = self.default.clone();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Global,
    Game,
}

type ResolveFn<T, Gl, Ga> = fn(&Gl, &Ga) -> (T, Origin);
type SetFn<Gl, Ga, S> = fn(&mut Gl, &mut Ga, S, Origin);

pub struct Overridable<T, Gl: Clone = T, Ga: Clone + Default = Option<Gl>, S = T> {
    global: Gl,
    default_global: Gl,
    game: Ga,
    default_game: Ga,
    unset_game: Ga,
    resolved: T,
    origin: Origin,

    resolve: ResolveFn<T, Gl, Ga>,
    set: SetFn<Gl, Ga, S>,
}

impl<T, Gl: Clone, Ga: Clone + Default, S> Overridable<T, Gl, Ga, S> {
    pub(super) fn new(
        global: Gl,
        default_global: Gl,
        game: Ga,
        default_game: Ga,
        unset_game: Ga,
        resolve: ResolveFn<T, Gl, Ga>,
        set: SetFn<Gl, Ga, S>,
    ) -> Self {
        let (resolved, origin) = resolve(&global, &game);
        Overridable {
            global,
            default_global,
            game,
            default_game,
            unset_game,
            resolved,
            origin,

            resolve,
            set,
        }
    }

    fn resolve(&mut self) {
        (self.resolved, self.origin) = (self.resolve)(&self.global, &self.game);
    }

    pub fn global(&self) -> &Gl {
        &self.global
    }

    pub fn update_global(&mut self, f: impl FnOnce(&mut Gl)) {
        f(&mut self.global);
        self.resolve();
    }

    pub fn set_global(&mut self, value: Gl) {
        self.global = value;
        self.resolve();
    }

    pub fn default_global(&self) -> &Gl {
        &self.default_global
    }

    pub fn set_default_global(&mut self) {
        self.global = self.default_global.clone();
        self.resolve();
    }

    pub fn game(&self) -> &Ga {
        &self.game
    }

    pub fn update_game(&mut self, f: impl FnOnce(&mut Ga)) {
        f(&mut self.game);
        self.resolve();
    }

    pub fn set_game(&mut self, value: Ga) {
        self.game = value;
        self.resolve();
    }

    pub fn default_game(&self) -> &Ga {
        &self.default_game
    }

    pub fn set_default_game(&mut self) {
        self.game = self.default_game.clone();
        self.resolve();
    }

    pub fn unset_game(&mut self) {
        self.game = self.unset_game.clone();
        self.resolve();
    }
}

pub trait OverridableTypes {
    type Global;
    type Game;
}

impl<T, Gl: Clone, Ga: Clone + Default, S> OverridableTypes for Overridable<T, Gl, Ga, S> {
    type Global = Gl;
    type Game = Ga;
}

impl<T, Gl: Clone, Ga: Clone + Default, S> Resolvable for Overridable<T, Gl, Ga, S> {
    type Resolved = T;
    type Set = S;

    fn get(&self) -> &Self::Resolved {
        &self.resolved
    }

    fn set(&mut self, value: Self::Set) {
        (self.set)(&mut self.global, &mut self.game, value, self.origin);
        self.resolve();
    }
}

pub trait Setting {
    type T: Resolvable;

    fn inner(&self) -> &Self::T;

    fn inner_mut(&mut self) -> &mut Self::T;

    fn get(&self) -> &<Self::T as Resolvable>::Resolved {
        self.inner().get()
    }

    fn set(&mut self, value: <Self::T as Resolvable>::Set) {
        self.inner_mut().set(value);
    }

    fn set_default(&mut self)
    where
        Self::T: Defaultable,
    {
        self.inner_mut().set_default();
    }
}

pub struct Untracked<T: Resolvable> {
    inner: T,
}

impl<T: Resolvable> Untracked<T> {
    pub(super) fn new(inner: T) -> Self {
        Untracked { inner }
    }
}

impl<T: Resolvable> Setting for Untracked<T> {
    type T = T;

    fn inner(&self) -> &Self::T {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Self::T {
        &mut self.inner
    }
}

pub struct Tracked<T: Resolvable> {
    inner: T,
    changed: bool,
}

impl<T: Resolvable> Tracked<T> {
    pub(super) fn new(inner: T) -> Self {
        Tracked {
            inner,
            changed: false,
        }
    }

    pub fn changed(&self) -> bool {
        self.changed
    }

    pub fn clear_updates(&mut self) {
        self.changed = false;
    }
}

impl<T: Resolvable> Setting for Tracked<T> {
    type T = T;

    fn inner(&self) -> &Self::T {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Self::T {
        self.changed = true;
        &mut self.inner
    }
}

pub fn resolve_option<T: Clone>(global: &T, game: &Option<T>) -> (T, Origin) {
    match game {
        Some(game) => (game.clone(), Origin::Game),
        _ => (global.clone(), Origin::Global),
    }
}

pub fn set_option<T: Clone>(global: &mut T, game: &mut Option<T>, value: T, origin: Origin) {
    if origin == Origin::Game {
        *game = Some(value.clone());
    }
    *global = value;
}

pub fn set_unreachable<Gl: Clone, Ga: Clone, S>(_: &mut Gl, _: &mut Ga, _: S, _: Origin) {
    unreachable!();
}
