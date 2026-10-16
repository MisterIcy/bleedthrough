<?php

function handle(string $value): void
{
    header('Location: ' . $value);
}
