"""Declarative extraction: fill dataclasses from HTML with CSS selectors.

Pure Python on top of the native ``Document``/``Element`` bindings; the Python
counterpart of the Rust ``#[derive(FromHtml)]`` (``dom-tree-derive``), with the
same semantics and error messages::

    @dataclass
    class Product:
        title: Annotated[str, Select("h1")]
        price: Annotated[float, Select(".price", attr="data-value")]

    extract(Product, html)

Each dataclass is analysed once (type hints resolved with
``typing.get_type_hints(..., include_extras=True)``, selectors validated) and
the resulting plan is cached per class.
"""

import dataclasses
import json
import threading
import types
import typing
import weakref
from typing import TYPE_CHECKING, Any, Callable, Dict, List, Optional, Sequence, Tuple, TypeVar, Union

if TYPE_CHECKING:
    from . import Document, Element, parse
else:
    from ._domtree import Document, Element, parse

# `Optional[X]` / `Union[X, None]`, and `X | None` (a `types.UnionType` before
# Python 3.14; that type does not exist on 3.9).
_UNION_ORIGINS: Tuple[Any, ...] = (Union, getattr(types, "UnionType", Union))

__all__ = ["Attr", "ExtractError", "Html", "Markdown", "OuterHtml", "Select", "extract", "extract_all"]

_T = TypeVar("_T")
_Scope = Union[Document, Element]
_MISSING: Any = dataclasses.MISSING
# Where a field's value comes from.
_TEXT, _HTML, _OUTER_HTML, _MARKDOWN = "text", "html", "outer_html", "markdown"
# Not a value source: a nested dataclass field (or `__select__`) selects elements.
_ELEMENT = "element"


# --- markers ---------------------------------------------------------------------


class Select:
    """Where a dataclass field's value comes from. Use it as
    ``Annotated[T, Select(...)]`` metadata.

    ``selector`` picks the element (the first match; every match for
    ``list[T]``); without one the field reads the scope itself (the element or
    document being extracted). The value is the element's readable text unless
    one of ``attr=`` (an attribute), ``html=True`` (inner HTML),
    ``outer_html=True`` or ``markdown=True`` says otherwise. ``default=`` is used
    when nothing matches or the attribute is absent; ``with_=`` converts the
    source string itself (its result is used as-is).
    """

    __slots__ = ("selector", "attr", "html", "outer_html", "markdown", "default", "with_")

    selector: Optional[str]
    attr: Optional[str]
    html: bool
    outer_html: bool
    markdown: bool
    default: Any
    with_: Optional[Callable[[str], Any]]

    def __init__(
        self,
        selector: Optional[str] = None,
        *,
        attr: Optional[str] = None,
        html: bool = False,
        outer_html: bool = False,
        markdown: bool = False,
        default: Any = _MISSING,
        with_: Optional[Callable[[str], Any]] = None,
    ) -> None:
        name = type(self).__name__
        if selector is not None and not isinstance(selector, str):
            raise TypeError(f"{name}(): selector must be a str or None, not {type(selector).__name__}")
        if attr is not None and not isinstance(attr, str):
            raise TypeError(f"{name}(): attr must be a str or None, not {type(attr).__name__}")
        if (attr is not None) + bool(html) + bool(outer_html) + bool(markdown) > 1:
            raise TypeError(f"{name}(): attr=, html=, outer_html= and markdown= are mutually exclusive")
        if with_ is not None and not callable(with_):
            raise TypeError(f"{name}(): with_ must be callable, not {type(with_).__name__}")
        self.selector = selector
        self.attr = attr
        self.html = bool(html)
        self.outer_html = bool(outer_html)
        self.markdown = bool(markdown)
        self.default = default
        self.with_ = with_

    def __repr__(self) -> str:
        args = [] if self.selector is None else [repr(self.selector)]
        if self.attr is not None:
            args.append(f"attr={self.attr!r}")
        for flag in ("html", "outer_html", "markdown"):
            if getattr(self, flag):
                args.append(f"{flag}=True")
        if self.default is not _MISSING:
            args.append(f"default={self.default!r}")
        if self.with_ is not None:
            args.append(f"with_={self.with_!r}")
        return f"Select({', '.join(args)})"


_NO_NAME: Any = object()


class Attr(Select):
    """``Attr(name)``: attribute ``name`` of the scope element;
    ``Attr(selector, name)``: of the first element matching ``selector``.
    Shorthand for ``Select(selector, attr=name)``."""

    __slots__ = ()

    def __init__(
        self,
        selector: Optional[str],
        name: Optional[str] = _NO_NAME,
        /,
        *,
        default: Any = _MISSING,
        with_: Optional[Callable[[str], Any]] = None,
    ) -> None:
        if name is _NO_NAME:  # Attr(name)
            selector, name = None, selector
        if not isinstance(name, str):
            raise TypeError(f"Attr(): the attribute name must be a str, not {type(name).__name__}")
        super().__init__(selector, attr=name, default=default, with_=with_)


class Html(Select):
    """Inner HTML of the match. Shorthand for ``Select(selector, html=True)``."""

    __slots__ = ()

    def __init__(
        self,
        selector: Optional[str] = None,
        *,
        default: Any = _MISSING,
        with_: Optional[Callable[[str], Any]] = None,
    ) -> None:
        super().__init__(selector, html=True, default=default, with_=with_)


class OuterHtml(Select):
    """Outer HTML of the match. Shorthand for ``Select(selector, outer_html=True)``."""

    __slots__ = ()

    def __init__(
        self,
        selector: Optional[str] = None,
        *,
        default: Any = _MISSING,
        with_: Optional[Callable[[str], Any]] = None,
    ) -> None:
        super().__init__(selector, outer_html=True, default=default, with_=with_)


class Markdown(Select):
    """The match as Markdown. Shorthand for ``Select(selector, markdown=True)``."""

    __slots__ = ()

    def __init__(
        self,
        selector: Optional[str] = None,
        *,
        default: Any = _MISSING,
        with_: Optional[Callable[[str], Any]] = None,
    ) -> None:
        super().__init__(selector, markdown=True, default=default, with_=with_)


# --- errors ----------------------------------------------------------------------


def _quote(s: str) -> str:
    """A double-quoted, escaped string (like Rust's ``{:?}``)."""
    return json.dumps(s, ensure_ascii=False)


class ExtractError(ValueError):
    """A field could not be extracted: no element matched, an attribute was
    missing, a value did not parse, or a selector is invalid.

    ``path`` names the field (``"Product.reviews[2].rating"``), ``selector``
    the CSS selector involved (``None`` when the field reads its scope), and
    ``source`` what was read: ``"text"``, ``"@attr"``, ``"html"``,
    ``"outer_html"``, ``"markdown"`` or ``"element"`` (a nested dataclass).
    """

    path: str
    selector: Optional[str]
    source: str
    reason: str

    def __init__(self, path: str, selector: Optional[str], source: str, reason: str) -> None:
        self.path = path
        self.selector = selector
        self.source = source
        self.reason = reason
        where = [] if selector is None else [_quote(selector)]
        if selector is None or source not in (_TEXT, _ELEMENT):
            where.append(source)
        super().__init__(f"{path} ({' '.join(where)}): {reason}")

    def __reduce__(self) -> Tuple[Any, ...]:
        return (type(self), (self.path, self.selector, self.source, self.reason))


# --- plans -----------------------------------------------------------------------

_ONE, _OPTIONAL, _LIST = 0, 1, 2

# An empty document, used to validate selectors when a plan is built.
_EMPTY = parse("")


def _read_text(e: Element) -> str:
    return e.text


def _read_html(e: Element) -> str:
    return e.inner_html


def _read_outer_html(e: Element) -> str:
    return e.html


def _read_markdown(e: Element) -> str:
    return e.to_markdown()


_ELEMENT_READERS: Dict[str, Callable[[Element], str]] = {
    _TEXT: _read_text,
    _HTML: _read_html,
    _OUTER_HTML: _read_outer_html,
    _MARKDOWN: _read_markdown,
}
_DOCUMENT_READERS: Dict[str, Callable[[Document], str]] = {
    _TEXT: Document.to_text,
    _HTML: Document.to_html,
    _OUTER_HTML: Document.to_html,
    _MARKDOWN: Document.to_markdown,
}


def _parse_bool(s: str) -> bool:
    t = s.strip().lower()
    if t == "true":
        return True
    if t == "false":
        return False
    raise ValueError('expected "true" or "false"')


def _parse_int(s: str) -> int:
    return int(s.strip())


def _parse_float(s: str) -> float:
    return float(s.strip())


_BUILTIN_PARSERS: Dict[Any, Callable[[str], Any]] = {int: _parse_int, float: _parse_float, bool: _parse_bool}


def _attr_reader(name: str) -> Callable[[Element], Optional[str]]:
    def read(e: Element) -> Optional[str]:
        return e.attr(name)

    return read


class _Field:
    """How to extract one dataclass field (copied out of its ``Select``, so a
    cached plan never changes)."""

    __slots__ = (
        "name",
        "selector",
        "source",
        "attr",
        "kind",
        "default",
        "nested",
        "convert",
        "trim",
        "verb",
        "read_element",
        "read_document",
    )

    def __init__(self, name: str, select: Select, kind: int) -> None:
        self.name = name
        self.selector = select.selector
        self.attr = select.attr
        self.kind = kind
        self.default: Any = select.default
        self.nested: Optional[_Plan] = None
        # `convert(raw)`; on failure the error reads "cannot <verb> <raw>: ...".
        self.convert: Optional[Callable[[str], Any]] = None
        self.trim = False
        self.verb = ""
        self.read_element: Callable[[Element], Optional[str]]
        self.read_document: Optional[Callable[[Document], str]] = None
        if select.attr is not None:
            self.source = "@" + select.attr
            self.read_element = _attr_reader(select.attr)
        else:
            self.source = _TEXT
            for source in (_HTML, _OUTER_HTML, _MARKDOWN):
                if getattr(select, source):
                    self.source = source
            self.read_element = _ELEMENT_READERS[self.source]
            self.read_document = _DOCUMENT_READERS[self.source]


class _Plan:
    __slots__ = ("cls", "name", "fields", "select")

    def __init__(self, cls: type) -> None:
        self.cls = cls
        self.name = cls.__name__
        self.fields: List[_Field] = []
        self.select: Optional[str] = None


# Plans keyed by class. Classes are weakly referenced, so a plan goes away with
# its class (unless the class refers to itself).
_PLANS: "weakref.WeakKeyDictionary[type, _Plan]" = weakref.WeakKeyDictionary()
_PLANS_LOCK = threading.RLock()


def _plan_for(cls: type) -> _Plan:
    if not (isinstance(cls, type) and dataclasses.is_dataclass(cls)):
        raise TypeError(f"expected a dataclass type, got {cls!r}")
    plan = _PLANS.get(cls)
    if plan is not None:
        return plan
    with _PLANS_LOCK:
        plan = _PLANS.get(cls)
        if plan is not None:
            return plan
        pending: Dict[type, _Plan] = {}
        plan = _build(cls, pending)
        # Only cache once every class involved analysed cleanly.
        _PLANS.update(pending)
        return plan


def _is_optional(tp: Any) -> bool:
    origin = typing.get_origin(tp)
    if origin in _UNION_ORIGINS:
        args = typing.get_args(tp)
        return len(args) == 2 and type(None) in args
    return False


def _strip_none(tp: Any) -> Any:
    return next(a for a in typing.get_args(tp) if a is not type(None))


def _validate_selector(path: str, selector: str, source: str) -> None:
    try:
        _EMPTY.select_first(selector)
    except ValueError as exc:
        raise ExtractError(path, selector, source, str(exc)) from exc


def _build(cls: type, pending: Dict[type, _Plan]) -> _Plan:
    existing = pending.get(cls) or _PLANS.get(cls)
    if existing is not None:
        return existing  # recursive (or already analysed) dataclass
    plan = _Plan(cls)
    pending[cls] = plan

    select = getattr(cls, "__select__", None)
    if select is not None:
        if not isinstance(select, str):
            raise TypeError(f"{plan.name}.__select__ must be a str, not {type(select).__name__}")
        _validate_selector(plan.name, select, _ELEMENT)
        plan.select = select

    try:
        hints = typing.get_type_hints(cls, include_extras=True)
    except Exception as exc:
        hint = ""
        if isinstance(exc, NameError):
            hint = (
                " (with `from __future__ import annotations`, every class an annotation"
                " names must be defined at module level, not inside a function)"
            )
        raise TypeError(f"cannot resolve the type hints of {plan.name}: {exc}{hint}") from exc

    for f in dataclasses.fields(cls):
        field = _build_field(plan.name, f, hints.get(f.name, f.type), pending)
        if field is not None:
            plan.fields.append(field)
    return plan


def _build_field(
    owner: str, f: "dataclasses.Field[Any]", tp: Any, pending: Dict[type, _Plan]
) -> Optional[_Field]:
    path = f"{owner}.{f.name}"
    has_default = f.default is not dataclasses.MISSING or f.default_factory is not dataclasses.MISSING

    # `Optional[Annotated[T, Select(...)]]` as well as `Annotated[Optional[T], ...]`.
    kind = _ONE
    if _is_optional(tp) and typing.get_origin(_strip_none(tp)) is typing.Annotated:
        kind = _OPTIONAL
        tp = _strip_none(tp)
    markers: Sequence[Any] = ()
    if typing.get_origin(tp) is typing.Annotated:
        tp, *markers = typing.get_args(tp)
    for m in markers:
        if isinstance(m, type) and issubclass(m, Select):
            raise TypeError(f"{path}: use {m.__name__}(...) (an instance), not the {m.__name__} class")
    selects = [m for m in markers if isinstance(m, Select)]

    if not selects:
        if has_default or not f.init:
            return None  # not extracted: the dataclass fills it in
        raise TypeError(
            f"{path} has no Select(...) marker: annotate it as Annotated[{_type_name(tp)}, Select(...)]"
            " or give it a default"
        )
    if len(selects) > 1:
        raise TypeError(f"{path} has more than one Select(...) marker")
    if not f.init:
        raise TypeError(f"{path} is declared with init=False, so it cannot be extracted")
    select = selects[0]

    if _is_optional(tp):
        kind = _OPTIONAL
        tp = _strip_none(tp)
    if tp is list or tp is List:
        raise TypeError(f"{path}: give the list an item type, e.g. list[str]")
    if typing.get_origin(tp) is list:
        if kind == _OPTIONAL:
            raise TypeError(
                f"{path}: Optional[list[...]] is not supported; a list field is empty when nothing matches"
            )
        kind = _LIST
        (tp,) = typing.get_args(tp)
        if typing.get_origin(tp) is typing.Annotated:
            tp = typing.get_args(tp)[0]
        if _is_optional(tp) or typing.get_origin(tp) is list:
            raise TypeError(f"{path}: unsupported item type {_type_name(tp)}")

    field = _Field(f.name, select, kind)
    if field.default is _MISSING and has_default:
        field.default = _OMIT  # let the dataclass fill in its own default
    if field.selector is not None:
        _validate_selector(path, field.selector, field.source)

    if select.with_ is not None:
        with_ = select.with_
        field.convert = with_
        field.verb = "convert {} with " + (getattr(with_, "__qualname__", None) or repr(with_))
    elif isinstance(tp, type) and dataclasses.is_dataclass(tp):
        if field.source != _TEXT:
            raise TypeError(
                f"{path}: a nested dataclass is extracted from the matched element; "
                "drop attr=/html=/outer_html=/markdown= (or convert the string with with_=)"
            )
        field.source = _ELEMENT
        field.nested = _build(tp, pending)
    elif tp is str:
        field.convert = None
    elif tp in _BUILTIN_PARSERS:
        field.convert = _BUILTIN_PARSERS[tp]
        field.trim = True
        field.verb = "parse {} as " + tp.__name__
    # Any other class is built from the string (`Any` is a class on 3.11+).
    elif isinstance(tp, type) and typing.get_origin(tp) is None and tp is not Any:
        field.convert = tp
        field.verb = "parse {} as " + tp.__name__
    else:
        raise TypeError(
            f"{path}: unsupported field type {_type_name(tp)}; use str, int, float, bool, a dataclass, "
            "a class constructed from a string, or convert the string yourself with Select(..., with_=...)"
        )
    return field


def _type_name(tp: Any) -> str:
    if isinstance(tp, type) and typing.get_origin(tp) is None:
        return tp.__name__
    return repr(tp).replace("typing.", "")


# --- extraction ------------------------------------------------------------------

_OMIT: Any = object()


def _instantiate(plan: _Plan, scope: _Scope, path: str) -> Any:
    kwargs: Dict[str, Any] = {}
    for field in plan.fields:
        value = _field_value(field, scope, path + "." + field.name)
        if value is not _OMIT:
            kwargs[field.name] = value
    return plan.cls(**kwargs)


def _missing(field: _Field, path: str, reason: str) -> Any:
    if field.default is not _MISSING:
        return field.default
    if field.kind == _OPTIONAL:
        return None
    raise ExtractError(path, field.selector, field.source, reason)


def _field_value(field: _Field, scope: _Scope, path: str) -> Any:
    selector = field.selector
    if field.kind == _LIST:
        if selector is None:
            matches: List[_Scope] = [scope]
        else:
            matches = list(scope.select(selector))
            if not matches and field.default is not _MISSING:
                return field.default
        return [_value(field, m, f"{path}[{i}]", True) for i, m in enumerate(matches)]

    if selector is None:
        return _value(field, scope, path, False)
    node = scope.select_first(selector)
    if node is None:
        return _missing(field, path, "no element matches")
    return _value(field, node, path, False)


def _value(field: _Field, node: _Scope, path: str, item: bool) -> Any:
    if field.nested is not None:
        return _instantiate(field.nested, node, path)

    if isinstance(node, Element):
        raw = field.read_element(node)
        if raw is None:  # attribute absent
            reason = f'<{node.tag}> has no "{field.attr}" attribute'
            if item:
                raise ExtractError(path, field.selector, field.source, reason)
            return _missing(field, path, reason)
    elif field.read_document is not None:
        raw = field.read_document(node)
    else:
        raise ExtractError(
            path,
            field.selector,
            field.source,
            "a document has no attributes; give the field a selector or extract from an Element",
        )

    convert = field.convert
    if convert is None:
        return raw
    try:
        return convert(raw)
    except Exception as exc:
        shown = raw.strip() if field.trim else raw
        detail = str(exc) or type(exc).__name__
        # `replace`, not `format`: a converter's name may contain braces.
        reason = "cannot " + field.verb.replace("{}", _quote(shown), 1) + ": " + detail
        raise ExtractError(path, field.selector, field.source, reason) from exc


def _scope(source: Union[str, Document, Element], func: str) -> _Scope:
    if isinstance(source, str):
        return parse(source)
    if isinstance(source, (Document, Element)):
        return source
    raise TypeError(f"{func}() source must be a str, Document or Element, not {type(source).__name__}")


# --- public API ------------------------------------------------------------------


def extract(cls: "type[_T]", source: Union[str, Document, Element]) -> _T:
    """Fill dataclass ``cls`` from ``source``: an HTML string (parsed), a
    :class:`Document` or an :class:`Element`, which is the scope selectors run
    in. Raises :class:`ExtractError` when a field cannot be extracted and
    :class:`TypeError` when ``cls`` is not a usable dataclass."""
    plan = _plan_for(cls)
    result: _T = _instantiate(plan, _scope(source, "extract"), plan.name)
    return result


def extract_all(cls: "type[_T]", source: Union[str, Document, Element]) -> "list[_T]":
    """One ``cls`` per element matching ``cls.__select__``, in document order,
    each extracted with that element as its scope."""
    plan = _plan_for(cls)
    if plan.select is None:
        raise TypeError(
            f"extract_all() needs {plan.name}.__select__, the CSS selector of the elements to extract "
            '(e.g. __select__ = ".product"); or select them yourself and call extract() on each'
        )
    scope = _scope(source, "extract_all")
    return [_instantiate(plan, el, f"{plan.name}[{i}]") for i, el in enumerate(scope.select(plan.select))]


for _cls in (Select, Attr, Html, OuterHtml, Markdown, ExtractError):
    _cls.__module__ = "domtree"
del _cls
