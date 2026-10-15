<?php

$user = $session->user ?? throw new Exception('no user');
$f = fn() => throw new LogicException();
$x or throw new RuntimeException();
