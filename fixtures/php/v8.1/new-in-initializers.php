<?php

class Service
{
    public function __construct(private Logger $logger = new NullLogger()) {}
}

function g(Logger $l = new NullLogger(1)) {}
static $s = new Foo();
const C = new Bar();
#[Attr(new Baz())]
function h() {}
