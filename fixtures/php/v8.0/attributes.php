<?php

#[Attribute(Attribute::TARGET_CLASS)]
class Route
{
    public function __construct(public string $path = '/') {}
}

#[Route('/home'), Marker]
#[Another(name: 'x', flags: [1, 2])]
class HomeController
{
    #[Inject]
    public $service;

    #[Pure]
    public function index(#[SensitiveParameter] $secret) {}
}

$f = #[Marker] fn() => 1;
